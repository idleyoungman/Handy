use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::config::OverlayPosition;

const NUM_BARS: usize = 16;

#[derive(Debug, Clone, Copy)]
pub enum OverlayStatus {
    Recording,
    Transcribing,
    Processing,
}

impl OverlayStatus {
    fn label(self) -> &'static str {
        match self {
            OverlayStatus::Recording => "Recording",
            OverlayStatus::Transcribing => "Transcribing",
            OverlayStatus::Processing => "Processing",
        }
    }
}

#[derive(Debug)]
pub enum OverlayInput {
    Show,
    Hide,
    SetStatus(OverlayStatus),
    MicLevel(f32),
}

pub struct Overlay {
    visible: bool,
    status: OverlayStatus,
    mic_levels: Rc<[Cell<f32>; NUM_BARS]>,
    level_idx: usize,
    drawing_area: gtk::DrawingArea,
}

#[relm4::component(pub)]
impl SimpleComponent for Overlay {
    type Init = OverlayPosition;
    type Input = OverlayInput;
    type Output = ();

    view! {
        gtk::Window {
            set_decorated: false,
            set_resizable: false,
            set_title: Some("Handy Overlay"),
            #[watch]
            set_visible: model.visible,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 4,
                set_margin_top: 8,
                set_margin_bottom: 8,
                set_margin_start: 16,
                set_margin_end: 16,

                #[name = "drawing_area"]
                gtk::DrawingArea {
                    set_content_width: 200,
                    set_content_height: 40,
                },

                #[name = "status_label"]
                gtk::Label {
                    #[watch]
                    set_label: model.status.label(),
                },
            }
        }
    }

    fn init(
        position: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Apply layer shell before the window is realized.
        root.init_layer_shell();
        root.set_layer(Layer::Overlay);
        root.set_keyboard_mode(KeyboardMode::None);
        root.auto_exclusive_zone_enable();
        root.set_margin(Edge::Left, 0);
        root.set_margin(Edge::Right, 0);

        let (anchor_top, anchor_bottom) = match position {
            OverlayPosition::Bottom => (false, true),
            _ => (true, false),
        };
        root.set_anchor(Edge::Top, anchor_top);
        root.set_anchor(Edge::Bottom, anchor_bottom);
        root.set_anchor(Edge::Left, true);
        root.set_anchor(Edge::Right, true);

        let mic_levels: Rc<[Cell<f32>; NUM_BARS]> =
            Rc::new(std::array::from_fn(|_| Cell::new(0.0)));

        let mut model = Overlay {
            visible: false,
            status: OverlayStatus::Recording,
            mic_levels: Rc::clone(&mic_levels),
            level_idx: 0,
            drawing_area: gtk::DrawingArea::new(),
        };

        let widgets = view_output!();

        let levels_for_draw = Rc::clone(&mic_levels);
        widgets
            .drawing_area
            .set_draw_func(move |_, cr, width, height| {
                draw_bars(cr, width, height, &levels_for_draw);
            });

        // Store a clone of the realized DrawingArea widget so update() can
        // call queue_draw() on mic-level events without going through widgets.
        model.drawing_area = widgets.drawing_area.clone();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: OverlayInput, _sender: ComponentSender<Self>) {
        match msg {
            OverlayInput::Show => {
                self.visible = true;
            }
            OverlayInput::Hide => {
                self.visible = false;
                for cell in self.mic_levels.iter() {
                    cell.set(0.0);
                }
                self.level_idx = 0;
            }
            OverlayInput::SetStatus(status) => {
                self.status = status;
                self.visible = true;
            }
            OverlayInput::MicLevel(level) => {
                self.mic_levels[self.level_idx % NUM_BARS].set(level);
                self.level_idx += 1;
                self.drawing_area.queue_draw();
            }
        }
    }
}

fn draw_bars(cr: &gtk::cairo::Context, width: i32, height: i32, levels: &[Cell<f32>; NUM_BARS]) {
    let w = width as f64;
    let h = height as f64;
    let bar_w = w / NUM_BARS as f64;

    cr.set_source_rgba(0.05, 0.05, 0.05, 0.85);
    cr.paint().ok();

    cr.set_source_rgba(0.95, 0.40, 0.10, 0.90);
    for (i, cell) in levels.iter().enumerate() {
        let level = cell.get().clamp(0.0, 1.0) as f64;
        let bar_h = level * h;
        let x = i as f64 * bar_w + 1.0;
        let y = h - bar_h;
        cr.rectangle(x, y, bar_w - 2.0, bar_h);
    }
    cr.fill().ok();
}
