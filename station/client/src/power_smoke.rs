//! UI-driven power outage regression, only used with --smoke-power.
use crate::ui::Ui;
use station_server::{Command, Frame, WorkTarget};
pub fn panels(stage: u8, ui: &mut Ui) {
    ui.inventory = !matches!(stage, 1 | 4 | 7 | 10 | 12);
    ui.floor = matches!(stage, 1 | 4 | 10).then_some([-4, 2, 0]);
}
pub fn drive(
    stage: &mut u8,
    last_tick: &mut u64,
    f: &Frame,
    ui: &Ui,
    raw: &mut egui::RawInput,
    renderer: &mut horizon_station_render::Renderer,
) {
    if f.player.tick < 100 || f.player.tick < *last_tick + 15 || *stage >= 12 {
        return;
    }
    let tool = f.inventory.hands[f.inventory.active];
    let kind = tool.map(|id| f.items[id].kind);
    let plating = f
        .floors
        .iter()
        .any(|t| t.cell == [-4, 2, 0] && t.kind == station_sim::floor::FloorKind::Plating);
    let complete = match *stage {
        0 | 6 => kind == Some(station_sim::ItemKind::Crowbar),
        1 => plating && f.action.is_none(),
        2 | 5 | 8 => tool.is_none(),
        3 | 9 => kind == Some(station_sim::ItemKind::Wirecutters),
        4 => !f.doors[0].powered && f.action.is_none(),
        7 => f.doors[0].open,
        10 => f.doors[0].powered && f.action.is_none(),
        11 => true,
        _ => false,
    };
    if complete {
        if *stage == 4 {
            assert_eq!(f.lamps[1].energy, 0.);
            renderer.capture_next("station/evidence/power-dark.png");
        }
        if *stage == 7 {
            renderer.capture_next("station/evidence/power-pried.png");
        }
        if *stage == 10 {
            assert!(f.lamps[1].energy > 0.);
            renderer.capture_next("station/evidence/power-restored.png");
        }
        *stage += 1;
        *last_tick = f.player.tick;
        return;
    }
    if f.action.is_some() {
        return;
    }
    let wanted = match *stage {
        0 | 2 | 6 | 8 => Some(Command::Slot(11)),
        3 | 5 | 9 => Some(Command::Slot(8)),
        1 => tool.map(|tool| Command::Work {
            target: WorkTarget::LiftTile([-4, 2, 0]),
            tool,
        }),
        4 | 10 => tool.map(|tool| Command::Work {
            target: WorkTarget::Cable([-4, 2, 0]),
            tool,
        }),
        7 => tool.map(|tool| Command::Work {
            target: WorkTarget::Door(0),
            tool,
        }),
        _ => None,
    };
    if let Some(wanted) = wanted
        && let Some((_, rect)) = ui.targets.iter().find(|(c, _)| *c == wanted)
    {
        let pos = rect.center();
        raw.events.push(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            raw.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
        println!(
            "power GUI request phase {}: {:?}; hand {:?}",
            *stage, wanted, tool
        );
        *last_tick = f.player.tick;
    }
}
