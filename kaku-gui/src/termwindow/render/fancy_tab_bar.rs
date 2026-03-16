use crate::customglyph::*;
use crate::tabbar::{compute_tab_plain_title, TabBarItem, TabEntry};
use crate::termwindow::box_model::*;
use crate::termwindow::render::corners::*;

use crate::termwindow::render::window_buttons::window_button_element;
use crate::termwindow::{TabDragRenderInfo, TabDragVisualMode, UIItem, UIItemType};
use crate::utilsprites::RenderMetrics;
use config::{Dimension, DimensionContext, TabBarColors};
use std::rc::Rc;
use wezterm_font::LoadedFont;
use wezterm_term::color::{ColorAttribute, ColorPalette};
use window::{IntegratedTitleButtonAlignment, IntegratedTitleButtonStyle};

const X_BUTTON: &[Poly] = &[
    Poly {
        path: &[
            PolyCommand::MoveTo(BlockCoord::One, BlockCoord::Zero),
            PolyCommand::LineTo(BlockCoord::Zero, BlockCoord::One),
        ],
        intensity: BlockAlpha::Full,
        style: PolyStyle::Outline,
    },
    Poly {
        path: &[
            PolyCommand::MoveTo(BlockCoord::Zero, BlockCoord::Zero),
            PolyCommand::LineTo(BlockCoord::One, BlockCoord::One),
        ],
        intensity: BlockAlpha::Full,
        style: PolyStyle::Outline,
    },
];

const PLUS_BUTTON: &[Poly] = &[
    Poly {
        path: &[
            PolyCommand::MoveTo(BlockCoord::Frac(1, 2), BlockCoord::Zero),
            PolyCommand::LineTo(BlockCoord::Frac(1, 2), BlockCoord::One),
        ],
        intensity: BlockAlpha::Full,
        style: PolyStyle::Outline,
    },
    Poly {
        path: &[
            PolyCommand::MoveTo(BlockCoord::Zero, BlockCoord::Frac(1, 2)),
            PolyCommand::LineTo(BlockCoord::One, BlockCoord::Frac(1, 2)),
        ],
        intensity: BlockAlpha::Full,
        style: PolyStyle::Outline,
    },
];

impl crate::TermWindow {
    pub fn invalidate_fancy_tab_bar(&mut self) {
        self.fancy_tab_bar.take();
    }

    pub fn build_fancy_tab_bar(&self, palette: &ColorPalette) -> anyhow::Result<ComputedElement> {
        let tab_bar_height = self.tab_bar_pixel_height()?;
        let font = self.fonts.title_font()?;
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());
        let items = self.tab_bar.items();
        let colors = self
            .config
            .colors
            .as_ref()
            .and_then(|c| c.tab_bar.as_ref())
            .cloned()
            .unwrap_or_else(TabBarColors::default);

        let mut title_left_eles = vec![];
        let mut strip_left_eles = vec![];
        let mut title_right_eles = vec![];
        // Use terminal background for immersive tab bar that blends with content
        let term_bg = palette.background.to_linear();
        // Slightly lighter bg for inactive tabs (subtle presence)
        let inactive_pill_bg = palette.background.lighten_fixed(0.03).to_linear();
        // More prominent bg for active tab pill
        let hover_pill_bg = palette.background.lighten_fixed(0.06).to_linear();
        // More prominent bg for active tab pill
        let active_pill_bg = palette.background.lighten_fixed(0.18).to_linear();

        let bar_colors = ElementColors {
            border: BorderColor::default(),
            bg: term_bg.into(),
            text: if self.focused.is_some() {
                self.config.window_frame.active_titlebar_fg
            } else {
                self.config.window_frame.inactive_titlebar_fg
            }
            .to_linear()
            .into(),
        };
        let tab_bottom_padding = Dimension::Cells(0.25);

        // Calculate horizontal padding for tabs: generous spacing for a less cramped look
        let tab_padding_h = Dimension::Pixels((0.75 * metrics.cell_size.width as f32) + 6.0);

        let item_to_elem = |item: &TabEntry| -> Element {
            let element = Element::with_line(&font, &item.title, palette);

            let _bg_color = item
                .title
                .get_cell(0)
                .and_then(|c| match c.attrs().background() {
                    ColorAttribute::Default => None,
                    col => Some(palette.resolve_bg(col)),
                });
            let fg_color = item
                .title
                .get_cell(0)
                .and_then(|c| match c.attrs().foreground() {
                    ColorAttribute::Default => None,
                    col => Some(palette.resolve_fg(col)),
                });

            let new_tab = colors.new_tab();
            let new_tab_hover = colors.new_tab_hover();
            let _active_tab = colors.active_tab();

            match item.item {
                TabBarItem::RightStatus | TabBarItem::LeftStatus | TabBarItem::None => element
                    .item_type(UIItemType::TabBar(TabBarItem::None))
                    .line_height(Some(1.75))
                    .margin(BoxDimension {
                        left: Dimension::Cells(0.),
                        right: Dimension::Cells(0.),
                        top: Dimension::Cells(0.0),
                        bottom: Dimension::Cells(0.),
                    })
                    .padding(BoxDimension {
                        left: Dimension::Cells(0.5),
                        right: Dimension::Cells(0.),
                        top: Dimension::Cells(0.),
                        bottom: Dimension::Cells(0.),
                    })
                    .border(BoxDimension::new(Dimension::Pixels(0.)))
                    .colors(bar_colors.clone()),
                TabBarItem::NewTabButton => Element::new(
                    &font,
                    ElementContent::Poly {
                        line_width: metrics.underline_height.max(2),
                        poly: SizedPoly {
                            poly: PLUS_BUTTON,
                            width: Dimension::Pixels(metrics.cell_size.height as f32 / 2.),
                            height: Dimension::Pixels(metrics.cell_size.height as f32 / 2.),
                        },
                    },
                )
                .vertical_align(VerticalAlign::Middle)
                .item_type(UIItemType::TabBar(item.item.clone()))
                .margin(BoxDimension {
                    left: Dimension::Cells(0.5),
                    right: Dimension::Cells(0.),
                    top: Dimension::Cells(0.2),
                    bottom: Dimension::Cells(0.),
                })
                .padding(BoxDimension {
                    left: Dimension::Cells(0.5),
                    right: Dimension::Cells(0.5),
                    top: Dimension::Cells(0.2),
                    bottom: tab_bottom_padding,
                })
                .border(BoxDimension::new(Dimension::Pixels(1.)))
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: new_tab.bg_color.to_linear().into(),
                    text: new_tab.fg_color.to_linear().into(),
                })
                .hover_colors(Some(ElementColors {
                    border: BorderColor::default(),
                    bg: new_tab_hover.bg_color.to_linear().into(),
                    text: new_tab_hover.fg_color.to_linear().into(),
                })),
                TabBarItem::Tab { active, .. } if active => element
                    .vertical_align(VerticalAlign::Bottom)
                    .horizontal_align(HorizontalAlign::Center)
                    .item_type(UIItemType::TabBar(item.item.clone()))
                    .margin(BoxDimension {
                        left: Dimension::Cells(0.1),
                        right: Dimension::Cells(0.1),
                        top: Dimension::Cells(0.25),
                        bottom: Dimension::Cells(0.15),
                    })
                    .padding(BoxDimension {
                        left: tab_padding_h,
                        right: tab_padding_h,
                        top: Dimension::Cells(0.15),
                        bottom: Dimension::Cells(0.15),
                    })
                    .border(BoxDimension::new(Dimension::Pixels(1.)))
                    .border_corners(Some(Corners {
                        top_left: SizedPoly {
                            width: Dimension::Cells(0.5),
                            height: Dimension::Cells(0.5),
                            poly: TOP_LEFT_ROUNDED_CORNER,
                        },
                        top_right: SizedPoly {
                            width: Dimension::Cells(0.5),
                            height: Dimension::Cells(0.5),
                            poly: TOP_RIGHT_ROUNDED_CORNER,
                        },
                        bottom_left: SizedPoly {
                            width: Dimension::Cells(0.5),
                            height: Dimension::Cells(0.5),
                            poly: BOTTOM_LEFT_ROUNDED_CORNER,
                        },
                        bottom_right: SizedPoly {
                            width: Dimension::Cells(0.5),
                            height: Dimension::Cells(0.5),
                            poly: BOTTOM_RIGHT_ROUNDED_CORNER,
                        },
                    }))
                    .colors(ElementColors {
                        border: BorderColor::new(active_pill_bg),
                        bg: active_pill_bg.into(),
                        text: fg_color
                            .unwrap_or(palette.foreground)
                            .to_linear()
                            .into(),
                    }),
                TabBarItem::Tab { .. } => element
                    .vertical_align(VerticalAlign::Bottom)
                    .horizontal_align(HorizontalAlign::Center)
                    .item_type(UIItemType::TabBar(item.item.clone()))
                    .margin(BoxDimension {
                        left: Dimension::Cells(0.1),
                        right: Dimension::Cells(0.1),
                        top: Dimension::Cells(0.25),
                        bottom: Dimension::Cells(0.15),
                    })
                    .padding(BoxDimension {
                        left: tab_padding_h,
                        right: tab_padding_h,
                        top: Dimension::Cells(0.15),
                        bottom: Dimension::Cells(0.15),
                    })
                    .border(BoxDimension::new(Dimension::Pixels(1.)))
                    .border_corners(Some(Corners {
                        top_left: SizedPoly {
                            width: Dimension::Cells(0.5),
                            height: Dimension::Cells(0.5),
                            poly: TOP_LEFT_ROUNDED_CORNER,
                        },
                        top_right: SizedPoly {
                            width: Dimension::Cells(0.5),
                            height: Dimension::Cells(0.5),
                            poly: TOP_RIGHT_ROUNDED_CORNER,
                        },
                        bottom_left: SizedPoly {
                            width: Dimension::Cells(0.5),
                            height: Dimension::Cells(0.5),
                            poly: BOTTOM_LEFT_ROUNDED_CORNER,
                        },
                        bottom_right: SizedPoly {
                            width: Dimension::Cells(0.5),
                            height: Dimension::Cells(0.5),
                            poly: BOTTOM_RIGHT_ROUNDED_CORNER,
                        },
                    }))
                    .colors(ElementColors {
                        border: BorderColor::new(inactive_pill_bg),
                        bg: inactive_pill_bg.into(),
                        text: fg_color
                            .unwrap_or_else(|| {
                                let inactive_tab = colors.inactive_tab();
                                inactive_tab.fg_color.into()
                            })
                            .to_linear()
                            .into(),
                    })
                    .hover_colors(Some(ElementColors {
                        border: BorderColor::new(hover_pill_bg),
                        bg: hover_pill_bg.into(),
                        text: fg_color
                            .unwrap_or(palette.foreground)
                            .to_linear()
                            .into(),
                    })),
                TabBarItem::WindowButton(button) => window_button_element(
                    button,
                    self.window_state.contains(window::WindowState::MAXIMIZED),
                    &font,
                    &metrics,
                    &self.config,
                ),
            }
        };

        // Two-layer layout: title row + tab strip row
        let cell_height = metrics.cell_size.height as f32;
        let title_row_height = (cell_height * 1.4).ceil().max(28.0);
        let tab_strip_height = (tab_bar_height - title_row_height).max(cell_height);
        let border = self.get_os_border();
        let is_fullscreen = self.layout_is_effective_fullscreen();
        // Tab strip padding in logical points (DPI-aware)
        let tab_strip_padding_pt = 12.0f32;
        let pt_to_px = self.dimensions.dpi as f32 / 72.0;
        let tab_strip_h_padding_px = (tab_strip_padding_pt * pt_to_px).floor();
        let window_buttons_at_left = self
            .config
            .window_decorations
            .contains(window::WindowDecorations::INTEGRATED_BUTTONS)
            && (self.config.integrated_title_button_alignment
                == IntegratedTitleButtonAlignment::Left
                || self.config.integrated_title_button_style
                    == IntegratedTitleButtonStyle::MacOsNative);

        // Use Points (not Pixels) so the padding is DPI-aware.
        // On 2x Retina (dpi=144): 70pt * 144/72 = 140 physical px = 70 logical pts,
        // which clears the macOS traffic-light buttons (~65 logical pts).
        let title_left_padding = if is_fullscreen {
            Dimension::Pixels(self.content_left_inset())
        } else if window_buttons_at_left {
            if self.config.integrated_title_button_style == IntegratedTitleButtonStyle::MacOsNative
            {
                Dimension::Points(70.0)
            } else {
                Dimension::Pixels(0.0)
            }
        } else {
            Dimension::Cells(0.5)
        };

        let tab_strip_left_padding_px = if is_fullscreen {
            self.content_left_inset()
        } else {
            tab_strip_h_padding_px
        };
        let tab_strip_right_padding_px = tab_strip_h_padding_px;

        let active_tab_title = mux::Mux::get()
            .get_window(self.mux_window_id)
            .and_then(|window| {
                let active_idx = window.get_active_idx();
                let last_active_idx = window.get_last_active_idx();
                window.get_by_idx(active_idx).map(|tab| {
                    let panes = self.get_pos_panes_for_tab(tab);
                    let tab_info = crate::termwindow::TabInformation {
                        tab_index: active_idx,
                        tab_id: tab.tab_id(),
                        is_active: true,
                        is_last_active: last_active_idx
                            .map(|last_active| last_active == active_idx)
                            .unwrap_or(false),
                        active_pane: panes
                            .iter()
                            .find(|pane| pane.is_active)
                            .map(Self::pos_pane_to_pane_info),
                        window_id: self.mux_window_id,
                        tab_title: tab.get_title(),
                    };
                    compute_tab_plain_title(&tab_info)
                })
            })
            .unwrap_or_default();

        // Count only actual Tab items for width division.
        // NewTabButton is much smaller than a tab pill, so reserve its
        // estimated width separately instead of giving it an equal share.
        let cell_width_f = metrics.cell_size.width as f32;
        let mut actual_tab_count: f32 = 0.;
        let mut non_tab_strip_width: f32 = 0.;
        for item in items.iter() {
            match item.item {
                TabBarItem::Tab { .. } => actual_tab_count += 1.,
                TabBarItem::NewTabButton => {
                    // margin(0.5 cells) + border(1px*2) + padding(0.5 cells*2) + content(cell_height/2)
                    non_tab_strip_width += 1.5 * cell_width_f + cell_height / 2.0 + 2.0;
                }
                _ => {}
            }
        }
        // Per-tab total overhead: margins(0.1 cells*2) + paddings(tab_padding_h*2) + borders(1px*2)
        let per_tab_overhead =
            0.2 * cell_width_f + 2.0 * (0.75 * cell_width_f + 6.0) + 2.0;
        let available_for_tabs = self.dimensions.pixel_width as f32
            - (border.left + border.right).get() as f32
            - tab_strip_left_padding_px
            - tab_strip_right_padding_px
            - non_tab_strip_width;
        let max_tab_width =
            ((available_for_tabs / actual_tab_count.max(1.0)) - per_tab_overhead).max(0.);

        let drag_info = self.tab_drag_render_info();
        let mut tab_slot = 0usize;

        for item in items {
            match item.item {
                TabBarItem::LeftStatus => title_left_eles.push(item_to_elem(item)),
                TabBarItem::RightStatus => title_right_eles.push(item_to_elem(item)),
                TabBarItem::None => {}
                TabBarItem::WindowButton(_) => {
                    if self.config.integrated_title_button_alignment
                        == IntegratedTitleButtonAlignment::Left
                    {
                        title_left_eles.push(item_to_elem(item))
                    } else {
                        title_right_eles.push(item_to_elem(item))
                    }
                }
                TabBarItem::Tab { tab_idx, active } => {
                    // During drag: skip the dragged tab entirely.
                    if drag_info
                        .as_ref()
                        .map_or(false, |d| tab_idx == d.dragged_tab_idx)
                    {
                        continue;
                    }

                    // Insert a transparent spacer at the target slot.
                    if let Some(ref d) = drag_info {
                        if matches!(
                            d.mode,
                            TabDragVisualMode::Reorder { target_slot_idx }
                                if tab_slot == target_slot_idx
                        ) {
                            strip_left_eles.push(
                                Element::new(&font, ElementContent::Text("".to_string()))
                                    .min_width(Some(Dimension::Pixels(d.overlay_width_px)))
                                    .min_height(Some(Dimension::Pixels(tab_strip_height * 0.6)))
                                    .colors(bar_colors.clone()),
                            );
                        }
                    }

                    let mut elem = item_to_elem(item);
                    elem.max_width = Some(Dimension::Pixels(max_tab_width));
                    elem.min_width = Some(Dimension::Pixels(max_tab_width));
                    elem.content = match elem.content {
                        ElementContent::Text(_) => unreachable!(),
                        ElementContent::Poly { .. } => unreachable!(),
                        ElementContent::Children(mut kids) => {
                            if self.config.show_close_tab_button_in_tabs
                                && !self.config.use_fancy_tab_bar
                            {
                                kids.push(make_x_button(&font, &metrics, &colors, tab_idx, active));
                            }
                            ElementContent::Children(kids)
                        }
                    };
                    strip_left_eles.push(elem);
                    tab_slot += 1;
                }
                _ => strip_left_eles.push(item_to_elem(item)),
            }
        }

        // If target slot is at the very end, insert spacer after all tabs.
        if let Some(ref d) = drag_info {
            if matches!(
                d.mode,
                TabDragVisualMode::Reorder { target_slot_idx } if tab_slot <= target_slot_idx
            ) {
                strip_left_eles.push(
                    Element::new(&font, ElementContent::Text("".to_string()))
                        .min_width(Some(Dimension::Pixels(d.overlay_width_px)))
                        .min_height(Some(Dimension::Pixels(tab_strip_height * 0.6)))
                        .colors(bar_colors.clone()),
                );
            }
        }

        // ── Row 1: Title row (same level as traffic lights) ──
        let mut title_row_children = vec![];

        if !title_left_eles.is_empty() {
            title_row_children.push(
                Element::new(&font, ElementContent::Children(title_left_eles))
                    .vertical_align(VerticalAlign::Middle)
                    .colors(bar_colors.clone()),
            );
        }

        // Active tab title text
        if !active_tab_title.is_empty() {
            title_row_children.push(
                Element::new(&font, ElementContent::Text(active_tab_title))
                    .vertical_align(VerticalAlign::Middle)
                    .colors(ElementColors {
                        border: BorderColor::default(),
                        bg: term_bg.into(),
                        text: palette.foreground.to_linear().into(),
                    })
                    .padding(BoxDimension {
                        left: Dimension::Pixels(16.),
                        right: Dimension::Cells(0.),
                        top: Dimension::Pixels(14.),
                        bottom: Dimension::Cells(0.),
                    }),
            );
        }

        if !title_right_eles.is_empty() {
            title_row_children.push(
                Element::new(&font, ElementContent::Children(title_right_eles))
                    .vertical_align(VerticalAlign::Middle)
                    .colors(bar_colors.clone())
                    .float(Float::Right),
            );
        }

        let title_row = Element::new(&font, ElementContent::Children(title_row_children))
            .display(DisplayType::Block)
            .item_type(UIItemType::TabBar(TabBarItem::None))
            .min_height(Some(Dimension::Pixels(title_row_height)))
            .colors(bar_colors.clone())
            .padding(BoxDimension {
                left: title_left_padding,
                right: Dimension::Cells(0.),
                top: Dimension::Cells(0.),
                bottom: Dimension::Cells(0.),
            });

        // ── Row 2: Tab strip (all tab pills) ──
        let tab_strip_inner = vec![
            Element::new(&font, ElementContent::Children(strip_left_eles))
                .vertical_align(VerticalAlign::Bottom)
                .colors(bar_colors.clone())
                .zindex(1),
        ];

        let tab_strip_row = Element::new(&font, ElementContent::Children(tab_strip_inner))
            .display(DisplayType::Block)
            .item_type(UIItemType::TabBar(TabBarItem::None))
            .min_height(Some(Dimension::Pixels(tab_strip_height)))
            .colors(bar_colors.clone())
            .padding(BoxDimension {
                left: Dimension::Pixels(tab_strip_left_padding_px),
                right: Dimension::Pixels(tab_strip_right_padding_px),
                top: Dimension::Cells(0.),
                bottom: Dimension::Cells(0.),
            });

        // ── Assemble outer container with two rows ──
        let children = vec![title_row, tab_strip_row];
        let content = ElementContent::Children(children);

        let tabs = Element::new(&font, content)
            .display(DisplayType::Block)
            .item_type(UIItemType::TabBar(TabBarItem::None))
            .min_width(Some(Dimension::Pixels(self.dimensions.pixel_width as f32)))
            .min_height(Some(Dimension::Pixels(tab_bar_height)))
            .vertical_align(VerticalAlign::Bottom)
            .colors(bar_colors);

        // In fullscreen, start from 0 since left_padding already handles alignment
        let bounds_left = if is_fullscreen {
            0.0
        } else {
            border.left.get() as f32
        };
        let bounds_width = if is_fullscreen {
            self.dimensions.pixel_width as f32
        } else {
            self.dimensions.pixel_width as f32 - (border.left + border.right).get() as f32
        };

        let mut computed = self.compute_element(
            &LayoutContext {
                height: DimensionContext {
                    dpi: self.dimensions.dpi as f32,
                    pixel_max: self.dimensions.pixel_height as f32,
                    pixel_cell: metrics.cell_size.height as f32,
                },
                width: DimensionContext {
                    dpi: self.dimensions.dpi as f32,
                    pixel_max: self.dimensions.pixel_width as f32,
                    pixel_cell: metrics.cell_size.width as f32,
                },
                bounds: euclid::rect(bounds_left, 0., bounds_width, tab_bar_height),
                metrics: &metrics,
                gl_state: self.render_state.as_ref().unwrap(),
                zindex: 10,
            },
            &tabs,
        )?;

        computed.translate(euclid::vec2(
            0.,
            if self.config.tab_bar_at_bottom {
                self.dimensions.pixel_height as f32
                    - (computed.bounds.height() + border.bottom.get() as f32)
            } else {
                // Fancy top tab bars are laid out to cover the titlebar area
                // directly, so don't push them down by the OS top inset.
                0.0
            },
        ));

        Ok(computed)
    }

    pub fn paint_fancy_tab_bar(&self) -> anyhow::Result<Vec<UIItem>> {
        let computed = self.fancy_tab_bar.as_ref().ok_or_else(|| {
            anyhow::anyhow!("paint_fancy_tab_bar called but fancy_tab_bar is None")
        })?;
        let ui_items = computed.ui_items();

        let gl_state = self.render_state.as_ref().unwrap();
        self.render_element(&computed, gl_state, None)?;

        Ok(ui_items)
    }

    /// Build and paint a standalone element for the dragged tab overlay.
    pub fn paint_fancy_tab_drag_overlay(&mut self, info: &TabDragRenderInfo) -> anyhow::Result<()> {
        let font = self.fonts.title_font()?;
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());
        let palette = self.palette().clone();
        let colors = self
            .config
            .colors
            .as_ref()
            .and_then(|c| c.tab_bar.as_ref())
            .cloned()
            .unwrap_or_else(TabBarColors::default);

        // Find the dragged tab's entry.
        let dragged_entry = self.tab_bar.items().iter().find(|e| {
            matches!(
                e.item,
                TabBarItem::Tab { tab_idx, .. } if tab_idx == info.dragged_tab_idx
            )
        });
        let dragged_entry = match dragged_entry {
            Some(e) => e,
            None => return Ok(()),
        };
        let active = matches!(dragged_entry.item, TabBarItem::Tab { active: true, .. });

        // Build a minimal element for the dragged tab.
        let elem_content = Element::with_line(&font, &dragged_entry.title, &palette);
        let active_tab_colors = colors.active_tab();
        let inactive_tab_colors = colors.inactive_tab();
        let tab_colors = if active {
            &active_tab_colors
        } else {
            &inactive_tab_colors
        };

        let tab_padding_h = Dimension::Pixels((0.5 * metrics.cell_size.width as f32) + 4.0);
        let tab_bottom_padding = Dimension::Cells(0.25);

        let overlay_elem = elem_content
            .vertical_align(VerticalAlign::Bottom)
            .padding(BoxDimension {
                left: tab_padding_h,
                right: tab_padding_h,
                top: Dimension::Cells(0.2),
                bottom: tab_bottom_padding,
            })
            .border(BoxDimension::new(Dimension::Pixels(1.)))
            .border_corners(Some(Corners {
                top_left: SizedPoly {
                    width: Dimension::Cells(0.5),
                    height: Dimension::Cells(0.5),
                    poly: TOP_LEFT_ROUNDED_CORNER,
                },
                top_right: SizedPoly {
                    width: Dimension::Cells(0.5),
                    height: Dimension::Cells(0.5),
                    poly: TOP_RIGHT_ROUNDED_CORNER,
                },
                bottom_left: SizedPoly::none(),
                bottom_right: SizedPoly::none(),
            }))
            .colors(ElementColors {
                border: BorderColor::new(tab_colors.bg_color.to_linear()),
                bg: tab_colors.bg_color.to_linear().into(),
                text: tab_colors.fg_color.to_linear().into(),
            })
            .min_width(Some(Dimension::Pixels(info.overlay_width_px)))
            .display(DisplayType::Block)
            .zindex(100);

        let computed = self.compute_element(
            &LayoutContext {
                height: DimensionContext {
                    dpi: self.dimensions.dpi as f32,
                    pixel_max: self.dimensions.pixel_height as f32,
                    pixel_cell: metrics.cell_size.height as f32,
                },
                width: DimensionContext {
                    dpi: self.dimensions.dpi as f32,
                    pixel_max: self.dimensions.pixel_width as f32,
                    pixel_cell: metrics.cell_size.width as f32,
                },
                bounds: euclid::rect(
                    info.overlay_left_px,
                    info.overlay_top_px,
                    info.overlay_width_px,
                    info.overlay_height_px,
                ),
                metrics: &metrics,
                gl_state: self.render_state.as_ref().unwrap(),
                zindex: 100,
            },
            &overlay_elem,
        )?;

        let gl_state = self.render_state.as_ref().unwrap();
        self.render_element(&computed, gl_state, None)?;

        Ok(())
    }
}

fn make_x_button(
    font: &Rc<LoadedFont>,
    metrics: &RenderMetrics,
    colors: &TabBarColors,
    tab_idx: usize,
    active: bool,
) -> Element {
    Element::new(
        &font,
        ElementContent::Poly {
            line_width: metrics.underline_height.max(2),
            poly: SizedPoly {
                poly: X_BUTTON,
                width: Dimension::Pixels(metrics.cell_size.height as f32 / 2.),
                height: Dimension::Pixels(metrics.cell_size.height as f32 / 2.),
            },
        },
    )
    // Ensure that we draw our background over the
    // top of the rest of the tab contents
    .zindex(1)
    .vertical_align(VerticalAlign::Middle)
    .float(Float::Right)
    .item_type(UIItemType::CloseTab(tab_idx))
    .hover_colors({
        let inactive_tab_hover = colors.inactive_tab_hover();
        let active_tab = colors.active_tab();

        Some(ElementColors {
            border: BorderColor::default(),
            bg: (if active {
                inactive_tab_hover.bg_color
            } else {
                active_tab.bg_color
            })
            .to_linear()
            .into(),
            text: (if active {
                inactive_tab_hover.fg_color
            } else {
                active_tab.fg_color
            })
            .to_linear()
            .into(),
        })
    })
    .padding(BoxDimension {
        left: Dimension::Cells(0.25),
        right: Dimension::Cells(0.25),
        top: Dimension::Cells(0.25),
        bottom: Dimension::Cells(0.25),
    })
    .margin(BoxDimension {
        left: Dimension::Cells(0.5),
        right: Dimension::Cells(0.),
        top: Dimension::Cells(0.),
        bottom: Dimension::Cells(0.),
    })
}
