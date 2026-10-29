use std::sync::mpsc::Sender;

use crate::display::Frame;
use crate::feedback::Feedback;
use crate::{Action, Input, InputEvent, Mapping, MappingSet, Trigger};

pub type EventSender = Sender<(String, InputEvent)>;
pub type Waker = Box<dyn Fn() + Send + 'static>;

pub trait ScreenSink {
    fn size(&self) -> (usize, usize);
    fn count(&self) -> usize;
    fn connected(&self) -> bool;
    fn problem(&self) -> Option<String>;
    fn send(&self, frames: Vec<Frame>);
}

pub trait Controller {
    fn name(&self) -> &'static str;
    fn source(&self) -> &'static str;
    fn connected(&self) -> bool;
    fn default_mappings(&self) -> Vec<Mapping>;
    fn upgrade_mapping(&self, _mapping: &Mapping) -> Option<Mapping> {
        None
    }
    fn owns_input(&self, _input: &Input) -> bool {
        false
    }
    fn translate(&mut self, set: &MappingSet, event: &InputEvent) -> Vec<(Action, Trigger)> {
        set.map(event)
    }
    fn show(&mut self, _feedback: &Feedback) {}
    fn tick(&mut self) -> Vec<(Action, Trigger)> {
        Vec::new()
    }
    fn held_stem(&self, _deck: usize) -> Option<usize> {
        None
    }
    fn held_slot(&self, _deck: usize) -> Option<(usize, bool)> {
        None
    }
    fn busy(&self) -> bool {
        false
    }
    fn deck_layers(&self) -> Option<[usize; 2]> {
        None
    }
    fn screen_decks(&self) -> Option<[usize; 2]> {
        self.deck_layers()
    }
    fn motor_jogs(&self) -> bool {
        false
    }
    fn calibration_groups(&self) -> &'static [crate::calibration::Group] {
        &[]
    }
    fn calibrate(&self, _group: usize, _step: crate::calibration::Step) -> bool {
        false
    }
    fn audio(&self) -> Option<crate::profile::Audio> {
        None
    }
    fn screens(&self) -> Option<&dyn ScreenSink> {
        None
    }
    fn status(&self) -> Vec<String> {
        Vec::new()
    }
}

#[derive(Debug)]
pub struct Model {
    pub name: &'static str,
    pub start: fn(EventSender, Waker) -> Result<Box<dyn Controller>, String>,
}

pub fn models() -> &'static [Model] {
    &[
        Model {
            name: crate::s4mk3::NAME,
            start: crate::s4mk3::driver::start,
        },
        Model {
            name: crate::kontrol::s2mk3::NAME,
            start: crate::kontrol::s2mk3::start,
        },
        Model {
            name: crate::kontrol::s3::NAME,
            start: crate::kontrol::s3::start,
        },
        Model {
            name: crate::kontrol::mx2::NAME,
            start: crate::kontrol::mx2::start,
        },
        Model {
            name: crate::kontrol::z1::NAME,
            start: crate::kontrol::z1::start,
        },
        Model {
            name: crate::kontrol::s4mk2::NAME,
            start: crate::kontrol::s4mk2::start,
        },
        Model {
            name: crate::kontrol::s2mk2::NAME,
            start: crate::kontrol::s2mk2::start,
        },
        Model {
            name: crate::kontrol::x1mk2::NAME,
            start: crate::kontrol::x1mk2::start,
        },
        Model {
            name: crate::kontrol::x1mk3::NAME,
            start: crate::kontrol::x1mk3::start,
        },
        Model {
            name: crate::kontrol::f1::NAME,
            start: crate::kontrol::f1::start,
        },
        Model {
            name: crate::kontrol::z2::NAME,
            start: crate::kontrol::z2::start,
        },
    ]
}
