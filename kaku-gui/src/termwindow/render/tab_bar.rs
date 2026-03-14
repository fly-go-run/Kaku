use crate::quad::TripleLayerQuadAllocator;
use crate::tabbar::TabBarItem;
use crate::termwindow::render::{forces_opaque_kaku_tui_window_background, RenderScreenLineParams};
use crate::termwindow::{TabDragRenderInfo, UIItemType};
use crate::utilsprites::RenderMetrics;
use config::ConfigHandle;
use mux::renderable::RenderableDimensions;
use wezterm_term::color::ColorAttribute;
use window::color::LinearRgba;

impl crate::TermWindow {
    pub fn paint_tab_bar(&mut self, layers: &mut TripleLayerQuadAllocator) -> anyhow::Result<()> {
        let _border = self.get_os_border();
        let tab_bar_height = self.tab_bar_pixel_height()?;
        let tab_bar_y = if self.config.tab_bar_at_bottom {
            // Position tab bar at the very bottom for a "flush" appearance.
            // The tab bar renders its own background, covering the bottom area.
            ((self.dimensions.pixel_height as f32) - tab_bar_height).max(0.)
        } else {
            // Position tab bar at the very top (y=0) for a "flush" appearance.
            // The fancy tab bar renders its own background, so it will cover
            // the titlebar area completely.
            0.0
        };
        let panes = self.get_panes_to_render();
        let force_opaque_tab_bar_background = forces_opaque_kaku_tui_window_background(&panes);

        if self.config.use_fancy_tab_bar {
            if self.fancy_tab_bar.is_none() {
                let palette = self.palette().clone();
                let tab_bar = self.build_fancy_tab_bar(&palette)?;
                self.fancy_tab_bar.replace(tab_bar);
            }

            // In transparent mode, fill the tab bar area with a transparent
            // background so it blends consistently with the window.
            let window_is_transparent =
                !self.window_background.is_empty() || self.config.window_background_opacity != 1.0;
            if window_is_transparent && !force_opaque_tab_bar_background {
                let tab_bar_bg = if let Some(active) = self.get_active_pane_or_overlay() {
                    active
                        .palette()
                        .background
                        .to_linear()
                        .mul_alpha(self.config.window_background_opacity)
                } else {
                    self.palette()
                        .background
                        .to_linear()
                        .mul_alpha(self.config.window_background_opacity)
                };
                self.filled_rectangle(
                    layers,
                    0,
                    euclid::rect(
                        0.0,
                        tab_bar_y,
                        self.dimensions.pixel_width as f32,
                        tab_bar_height,
                    ),
                    tab_bar_bg,
                )?;
            }

            let mut fancy_ui_items = self.paint_fancy_tab_bar()?;
            let drag_info = self.tab_drag_render_info();
            if let Some(ref info) = drag_info {
                // Filter out the dragged tab's UIItems (tab and close button).
                fancy_ui_items.retain(|item| {
                    !matches!(
                        item.item_type,
                        UIItemType::TabBar(TabBarItem::Tab { tab_idx, .. })
                        if tab_idx == info.dragged_tab_idx
                    ) && !matches!(
                        item.item_type,
                        UIItemType::CloseTab(idx) if idx == info.dragged_tab_idx
                    )
                });

                // Paint the overlay for the dragged tab.
                self.paint_fancy_tab_drag_overlay(info, tab_bar_y, tab_bar_height)?;
            }
            self.ui_items.append(&mut fancy_ui_items);
            return Ok(());
        }

        let palette = self.palette().clone();
        let drag_info = self.tab_drag_render_info();

        let cell_height = self.render_metrics.cell_size.height as usize;
        let cell_width = self.render_metrics.cell_size.width as usize;

        // Register the tab bar location. During drag, retro mode uses the shifted
        // tab positions so hit-testing and subsequent slot computation match what
        // we actually render on screen.
        let mut tab_ui_items = if let Some(ref info) = drag_info {
            let tab_entries: Vec<_> = self
                .tab_bar
                .items()
                .iter()
                .filter(|entry| matches!(entry.item, TabBarItem::Tab { .. }))
                .collect();

            let mut items: Vec<_> = self
                .tab_bar
                .items()
                .iter()
                .filter(|entry| !matches!(entry.item, TabBarItem::Tab { .. }))
                .map(|entry| crate::termwindow::UIItem {
                    x: entry.x * cell_width,
                    width: entry.width * cell_width,
                    y: tab_bar_y as usize,
                    height: cell_height,
                    item_type: UIItemType::TabBar(entry.item),
                })
                .collect();

            if let Some(first_tab) = tab_entries.first() {
                let mut current_left = first_tab.x * cell_width;
                let mut tab_slot = 0usize;

                for entry in tab_entries {
                    if matches!(
                        entry.item,
                        TabBarItem::Tab { tab_idx, .. } if tab_idx == info.dragged_tab_idx
                    ) {
                        continue;
                    }

                    if tab_slot == info.target_slot_idx {
                        current_left += info.overlay_width_px as usize;
                    }

                    items.push(crate::termwindow::UIItem {
                        x: current_left,
                        width: entry.width * cell_width,
                        y: tab_bar_y as usize,
                        height: cell_height,
                        item_type: UIItemType::TabBar(entry.item),
                    });

                    current_left += entry.width * cell_width;
                    tab_slot += 1;
                }
            }

            items.sort_by_key(|item| item.x);
            items
        } else {
            self.tab_bar
                .compute_ui_items(tab_bar_y as usize, cell_height, cell_width)
        };
        self.ui_items.append(&mut tab_ui_items);

        let window_is_transparent =
            !self.window_background.is_empty() || self.config.window_background_opacity != 1.0;
        let effective_window_is_transparent =
            window_is_transparent && !force_opaque_tab_bar_background;
        let gl_state = self.render_state.as_ref().unwrap();
        let white_space = gl_state.util_sprites.white_space.texture_coords();
        let filled_box = gl_state.util_sprites.filled_box.texture_coords();
        let default_bg = palette
            .resolve_bg(ColorAttribute::Default)
            .to_linear()
            .mul_alpha(if effective_window_is_transparent {
                0.
            } else {
                self.config.text_background_opacity
            });

        let retro_drag_bg = if effective_window_is_transparent {
            if let Some(active) = self.get_active_pane_or_overlay() {
                active
                    .palette()
                    .background
                    .to_linear()
                    .mul_alpha(self.config.window_background_opacity)
            } else {
                palette
                    .background
                    .to_linear()
                    .mul_alpha(self.config.window_background_opacity)
            }
        } else {
            default_bg
        };

        if effective_window_is_transparent {
            self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    0.0,
                    tab_bar_y,
                    self.dimensions.pixel_width as f32,
                    tab_bar_height,
                ),
                retro_drag_bg,
            )?;
        }

        self.render_screen_line(
            RenderScreenLineParams {
                top_pixel_y: tab_bar_y,
                left_pixel_x: 0.,
                pixel_width: self.dimensions.pixel_width as f32,
                stable_line_idx: None,
                line: self.tab_bar.line(),
                selection: 0..0,
                cursor: &Default::default(),
                palette: &palette,
                dims: &RenderableDimensions {
                    cols: self.dimensions.pixel_width
                        / self.render_metrics.cell_size.width as usize,
                    physical_top: 0,
                    scrollback_rows: 0,
                    scrollback_top: 0,
                    viewport_rows: 1,
                    dpi: self.terminal_size.dpi,
                    pixel_height: self.render_metrics.cell_size.height as usize,
                    pixel_width: self.terminal_size.pixel_width,
                    reverse_video: false,
                },
                config: &self.config,
                cursor_border_color: LinearRgba::default(),
                foreground: palette.foreground.to_linear(),
                pane: None,
                is_active: true,
                selection_fg: LinearRgba::default(),
                selection_bg: LinearRgba::default(),
                cursor_fg: LinearRgba::default(),
                cursor_bg: LinearRgba::default(),
                cursor_is_default_color: true,
                white_space,
                filled_box,
                window_is_transparent: effective_window_is_transparent,
                default_bg,
                style: None,
                font: None,
                use_pixel_positioning: self.config.experimental_pixel_positioning,
                render_metrics: self.render_metrics,
                shape_key: None,
                password_input: false,
            },
            layers,
        )?;

        // ── Retro tab drag overlay ──
        if let Some(ref info) = drag_info {
            self.paint_retro_tab_drag_overlay(
                layers,
                info,
                tab_bar_y,
                tab_bar_height,
                &palette,
                retro_drag_bg,
                default_bg,
                effective_window_is_transparent,
                white_space,
                filled_box,
            )?;
        }

        Ok(())
    }

    /// Paint the retro drag overlay: erase the dragged tab from the base strip
    /// with a background rectangle, then render the tab title at the overlay position.
    fn paint_retro_tab_drag_overlay(
        &self,
        layers: &mut TripleLayerQuadAllocator,
        info: &TabDragRenderInfo,
        tab_bar_y: f32,
        tab_bar_height: f32,
        palette: &wezterm_term::color::ColorPalette,
        erase_bg: LinearRgba,
        default_bg: LinearRgba,
        window_is_transparent: bool,
        white_space: window::bitmaps::TextureRect,
        filled_box: window::bitmaps::TextureRect,
    ) -> anyhow::Result<()> {
        let cell_width = self.render_metrics.cell_size.width as f32;

        let tab_entries: Vec<_> = self
            .tab_bar
            .items()
            .iter()
            .filter(|entry| matches!(entry.item, TabBarItem::Tab { .. }))
            .collect();
        let Some(first_tab) = tab_entries.first() else {
            return Ok(());
        };
        let Some(last_tab) = tab_entries.last() else {
            return Ok(());
        };
        let dragged_entry = tab_entries.iter().find_map(|entry| match entry.item {
            TabBarItem::Tab { tab_idx, .. } if tab_idx == info.dragged_tab_idx => Some(*entry),
            _ => None,
        });
        let Some(dragged_entry) = dragged_entry else {
            return Ok(());
        };

        let strip_left = first_tab.x as f32 * cell_width;
        let strip_right = (last_tab.x + last_tab.width) as f32 * cell_width;

        // Repaint the whole tab strip background first; non-dragged tabs are then
        // re-rendered in their shifted positions, leaving the target slot blank.
        self.filled_rectangle(
            layers,
            1,
            euclid::rect(strip_left, tab_bar_y, strip_right - strip_left, tab_bar_height),
            erase_bg,
        )?;

        let mut current_left = strip_left;
        let mut tab_slot = 0usize;
        for entry in &tab_entries {
            if matches!(
                entry.item,
                TabBarItem::Tab { tab_idx, .. } if tab_idx == info.dragged_tab_idx
            ) {
                continue;
            }

            if tab_slot == info.target_slot_idx {
                current_left += info.overlay_width_px;
            }

            self.render_screen_line(
                RenderScreenLineParams {
                    top_pixel_y: tab_bar_y,
                    left_pixel_x: current_left,
                    pixel_width: entry.width as f32 * cell_width,
                    stable_line_idx: None,
                    line: &entry.title,
                    selection: 0..0,
                    cursor: &Default::default(),
                    palette,
                    dims: &RenderableDimensions {
                        cols: entry.width,
                        physical_top: 0,
                        scrollback_rows: 0,
                        scrollback_top: 0,
                        viewport_rows: 1,
                        dpi: self.terminal_size.dpi,
                        pixel_height: self.render_metrics.cell_size.height as usize,
                        pixel_width: entry.width * self.render_metrics.cell_size.width as usize,
                        reverse_video: false,
                    },
                    config: &self.config,
                    cursor_border_color: LinearRgba::default(),
                    foreground: palette.foreground.to_linear(),
                    pane: None,
                    is_active: true,
                    selection_fg: LinearRgba::default(),
                    selection_bg: LinearRgba::default(),
                    cursor_fg: LinearRgba::default(),
                    cursor_bg: LinearRgba::default(),
                    cursor_is_default_color: true,
                    white_space,
                    filled_box,
                    window_is_transparent,
                    default_bg,
                    style: None,
                    font: None,
                    use_pixel_positioning: self.config.experimental_pixel_positioning,
                    render_metrics: self.render_metrics,
                    shape_key: None,
                    password_input: false,
                },
                layers,
            )?;

            current_left += entry.width as f32 * cell_width;
            tab_slot += 1;
        }

        // Render the dragged tab title at the overlay position last so it stays on top.
        self.render_screen_line(
            RenderScreenLineParams {
                top_pixel_y: tab_bar_y,
                left_pixel_x: info.overlay_left_px,
                pixel_width: info.overlay_width_px,
                stable_line_idx: None,
                line: &dragged_entry.title,
                selection: 0..0,
                cursor: &Default::default(),
                palette,
                dims: &RenderableDimensions {
                    cols: dragged_entry.width,
                    physical_top: 0,
                    scrollback_rows: 0,
                    scrollback_top: 0,
                    viewport_rows: 1,
                    dpi: self.terminal_size.dpi,
                    pixel_height: self.render_metrics.cell_size.height as usize,
                    pixel_width: info.overlay_width_px as usize,
                    reverse_video: false,
                },
                config: &self.config,
                cursor_border_color: LinearRgba::default(),
                foreground: palette.foreground.to_linear(),
                pane: None,
                is_active: true,
                selection_fg: LinearRgba::default(),
                selection_bg: LinearRgba::default(),
                cursor_fg: LinearRgba::default(),
                cursor_bg: LinearRgba::default(),
                cursor_is_default_color: true,
                white_space,
                filled_box,
                window_is_transparent,
                default_bg,
                style: None,
                font: None,
                use_pixel_positioning: self.config.experimental_pixel_positioning,
                render_metrics: self.render_metrics,
                shape_key: None,
                password_input: false,
            },
            layers,
        )?;

        Ok(())
    }

    pub fn tab_bar_pixel_height_impl(
        config: &ConfigHandle,
        fontconfig: &wezterm_font::FontConfiguration,
        render_metrics: &RenderMetrics,
    ) -> anyhow::Result<f32> {
        if config.use_fancy_tab_bar {
            let font = fontconfig.title_font()?;
            Ok((font.metrics().cell_height.get() as f32 * 1.75).ceil())
        } else {
            Ok(render_metrics.cell_size.height as f32)
        }
    }

    pub fn tab_bar_pixel_height(&self) -> anyhow::Result<f32> {
        Self::tab_bar_pixel_height_impl(&self.config, &self.fonts, &self.render_metrics)
    }
}
