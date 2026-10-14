//! A tower seen from the side, one storey per herdr workspace stacked on a
//! lobby, with a desk per agent. Each agent is a mascot behind its desk,
//! looping the animation for its state: running when it works, asleep when
//! idle, waving with "!" when blocked, hopping when done, "?" when unknown. A
//! storey's lights are off when all its agents sleep. The elevator rides to
//! the focused agent's storey and a plumbob floats over that agent. Tapping an
//! agent asks the bridge to focus it.

use alloc::vec::Vec;
use herdr_panel_art::Behavior;
use herdr_panel_protocol::{Agent, Id, State, Status, ToHost};
use oxivgl::{
    anim::{ANIM_REPEAT_INFINITE, Anim, anim_path_ease_in_out, anim_set_y},
    enums::EventCode,
    event::Event,
    indev::PointerIndev,
    view::{NavAction, View, register_event_on},
    widgets::{AnimImg, AsLvHandle, Image, Obj, Screen, WidgetError},
};

use crate::{
    frames::{
        CAR_POS, DESKS_PER_FLOOR, FLOOR_POS, Loop, MAX_FLOORS, ROOF_POS, SLOTS, car_image, desk_image, floor_image, loops,
        plumbob, roof, tower_image,
    },
    link,
    touch::POINTER,
};

/// Width of the plumbob, and how far its top is above the agent's feet, in
/// screen pixels.
const PLUMBOB_W: i32 = 10;
const PLUMBOB_ABOVE: i32 = 38;
/// The tappable area around an agent's feet.
const HIT_W: i32 = 44;
const HIT_ABOVE: i32 = 36;
/// How long the elevator takes per storey, in milliseconds.
const CAR_MS_PER_LEVEL: u32 = 250;

fn behavior(status: Status) -> Behavior {
    match status {
        Status::Working => Behavior::Work,
        Status::Idle => Behavior::Sleep,
        Status::Blocked => Behavior::Blocked,
        Status::Done => Behavior::Done,
        Status::Unknown => Behavior::Unknown,
    }
}

/// The agents grouped into storeys by workspace, keeping their order: the
/// workspace is the part of the pane id before ':'. At most [`MAX_FLOORS`]
/// storeys of [`DESKS_PER_FLOOR`] agents; the rest are left out.
fn storeys(agents: &[Agent]) -> Vec<Vec<&Agent>> {
    fn workspace(a: &Agent) -> &str {
        a.id.split(':').next().unwrap_or_default()
    }
    let mut out: Vec<Vec<&Agent>> = Vec::new();
    for agent in agents {
        let same = out.last().is_some_and(|s| workspace(s[0]) == workspace(agent));
        if !same {
            if out.len() == MAX_FLOORS {
                break;
            }
            out.push(Vec::new());
        }
        let storey = out.last_mut().expect("pushed above");
        if storey.len() < DESKS_PER_FLOOR {
            storey.push(agent);
        }
    }
    out
}

/// The widgets built for one state; each is a child of the screen.
#[derive(Default)]
struct Rendered {
    floors: Vec<Image<'static>>,
    sprites: Vec<AnimImg<'static>>,
    desks: Vec<Image<'static>>,
    /// Transparent tappable areas, one per agent, on top of everything.
    hits: Vec<(Obj<'static>, Id)>,
}

pub struct Panel {
    indev: Option<PointerIndev>,
    tower: Option<Image<'static>>,
    car: Option<Image<'static>>,
    /// The level the car is at or heading for.
    car_level: usize,
    rendered: Rendered,
    loops: Vec<(Behavior, Loop)>,
    plumbob: Option<Loop>,
    roof: Option<Loop>,
    shown: u32,
}

impl Panel {
    pub fn new() -> Self {
        Self {
            indev: None,
            tower: None,
            car: None,
            car_level: 0,
            rendered: Rendered::default(),
            loops: Vec::new(),
            plumbob: None,
            roof: None,
            shown: 0,
        }
    }

    fn looping(&self, parent: &impl AsLvHandle, l: &Loop, x: i32, y: i32) -> Result<AnimImg<'static>, WidgetError> {
        let img = AnimImg::new(parent)?;
        img.pos(x, y);
        img.set_src(l.frames).set_duration(l.duration_ms).set_repeat_count(ANIM_REPEAT_INFINITE).start();
        Ok(img)
    }

    fn render(&mut self, container: &impl AsLvHandle, state: &State) -> Result<(), WidgetError> {
        self.rendered = Rendered::default();
        let mut r = Rendered::default();
        let storeys = storeys(&state.agents);

        for (k, agents) in storeys.iter().enumerate() {
            let level = k + 1;
            let lit = agents.iter().any(|a| a.status != Status::Idle);
            let floor = Image::new(container)?;
            floor.set_src(floor_image(level, lit));
            floor.pos(FLOOR_POS[level].0, FLOOR_POS[level].1);
            r.floors.push(floor);
        }
        if let Some(l) = &self.roof {
            let (x, y) = ROOF_POS[storeys.len()];
            r.sprites.push(self.looping(container, l, x, y)?);
        }

        let mut focused_level = 0;
        for (k, agents) in storeys.iter().enumerate() {
            for (i, agent) in agents.iter().enumerate() {
                let slot = &SLOTS[k][i];
                if let Some((_, l)) = self.loops.iter().find(|(b, _)| *b == behavior(agent.status)) {
                    r.sprites.push(self.looping(container, l, slot.mascot.0, slot.mascot.1)?);
                }
                let desk = Image::new(container)?;
                desk.set_src(desk_image());
                desk.pos(slot.desk.0, slot.desk.1);
                r.desks.push(desk);
                if agent.focused {
                    focused_level = k + 1;
                    if let Some(l) = &self.plumbob {
                        let (fx, fy) = slot.feet;
                        r.sprites.push(self.looping(container, l, fx - PLUMBOB_W / 2, fy - PLUMBOB_ABOVE)?);
                    }
                }
                let hit = Obj::new(container)?;
                hit.remove_style_all();
                hit.size(HIT_W, HIT_ABOVE + 8).pos(slot.feet.0 - HIT_W / 2, slot.feet.1 - HIT_ABOVE);
                hit.bubble_events();
                r.hits.push((hit, agent.id.clone()));
            }
        }
        self.rendered = r;
        self.ride_to(focused_level);
        Ok(())
    }

    /// Keeps the car in front of the rebuilt storeys and sends it to `level`.
    fn ride_to(&mut self, level: usize) {
        let Some(car) = &self.car else { return };
        car.move_foreground();
        for (hit, _) in &self.rendered.hits {
            hit.move_foreground();
        }
        if level == self.car_level {
            return;
        }
        let from = car.get_y();
        let to = CAR_POS[level].1;
        let mut a = Anim::new();
        a.set_var(car)
            .set_values(from, to)
            .set_duration(CAR_MS_PER_LEVEL * self.car_level.abs_diff(level) as u32)
            .set_path_cb(Some(anim_path_ease_in_out))
            .set_exec_cb(Some(anim_set_y));
        a.start();
        self.car_level = level;
    }
}

impl View for Panel {
    fn create(&mut self, container: &Obj<'static>) -> Result<(), WidgetError> {
        self.indev = Some(PointerIndev::new(&POINTER)?);
        self.loops = loops().into_iter().collect();
        self.plumbob = Some(plumbob());
        self.roof = Some(roof());
        container.bg_color(0x000000).bg_opa(255);
        let tower = Image::new(container)?;
        tower.set_src(tower_image());
        tower.pos(0, 0);
        self.tower = Some(tower);

        let car = Image::new(container)?;
        car.set_src(car_image());
        car.pos(CAR_POS[0].0, CAR_POS[0].1);
        self.car = Some(car);

        // Until the bridge answers: the lobby under an empty roof.
        self.render(container, &State::default())
    }

    fn update(&mut self) -> Result<NavAction, WidgetError> {
        let Some((version, state)) = link::newer_than(self.shown) else { return Ok(NavAction::None) };
        self.shown = version;
        let Some(screen) = Screen::active() else { return Ok(NavAction::None) };
        self.render(&screen, &state)?;
        let handles: Vec<_> = self.rendered.hits.iter().map(|(h, _)| h.handle()).collect();
        for h in handles {
            register_event_on(self, h);
        }
        Ok(NavAction::None)
    }

    fn on_event(&mut self, event: &Event) -> NavAction {
        for (hit, id) in &self.rendered.hits {
            if event.matches(hit, EventCode::CLICKED) {
                link::send(&ToHost::Focus { agent: id.clone() });
            }
        }
        NavAction::None
    }
}
