// The artist -> album -> track navigation stack: each frame is one
// on-screen list, entering an item pushes the next level's list, and B pops
// back to the previous one.

use crate::api::{Client, Item};

enum Level {
    Artists,
    Albums,
    Tracks,
}

pub struct Frame {
    level: Level,
    pub title: String,
    pub items: Vec<Item>,
    pub selected: usize,
}

impl Frame {
    // Pure and Client-free, so the wrap-around behavior is unit-testable
    // without a live server (see the tests below).
    fn move_selection(&mut self, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let len = self.items.len() as i32;
        self.selected = (self.selected as i32 + delta).rem_euclid(len) as usize;
    }
}

pub struct Browser {
    client: Client,
    stack: Vec<Frame>,
}

impl Browser {
    // Takes ownership of the client: nothing else needs it once browsing
    // starts, and owning it outright (rather than borrowing) keeps Browser
    // and everything built on it free of a lifetime parameter.
    pub fn new(client: Client) -> Self {
        let items = client.list_artists();
        let root = Frame { level: Level::Artists, title: "Artists".to_string(), items, selected: 0 };
        Browser { client, stack: vec![root] }
    }

    pub fn stream_url(&self, item_id: &str) -> String {
        self.client.stream_url(item_id)
    }

    pub fn current(&self) -> &Frame {
        self.stack.last().expect("browser stack is never empty")
    }

    pub fn move_selection(&mut self, delta: i32) {
        self.stack.last_mut().expect("browser stack is never empty").move_selection(delta);
    }

    // Enters the selected item: drills into the next level and returns
    // None, or — already at track level — returns the track to play.
    pub fn enter(&mut self) -> Option<Item> {
        let frame = self.stack.last().expect("browser stack is never empty");
        let item = frame.items.get(frame.selected)?;
        match frame.level {
            Level::Artists => {
                let items = self.client.list_albums(&item.id);
                self.stack.push(Frame { level: Level::Albums, title: item.name.clone(), items, selected: 0 });
                None
            }
            Level::Albums => {
                let items = self.client.list_tracks(&item.id);
                self.stack.push(Frame { level: Level::Tracks, title: item.name.clone(), items, selected: 0 });
                None
            }
            Level::Tracks => Some(item.clone()),
        }
    }

    // Pops back one level; false if already at the root.
    pub fn back(&mut self) -> bool {
        if self.stack.len() <= 1 {
            return false;
        }
        self.stack.pop();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(len: usize) -> Frame {
        let items = (0..len).map(|i| Item { id: i.to_string(), name: i.to_string() }).collect();
        Frame { level: Level::Artists, title: String::new(), items, selected: 0 }
    }

    #[test]
    fn move_selection_wraps_past_the_end() {
        let mut f = frame(3);
        f.selected = 2;
        f.move_selection(1);
        assert_eq!(f.selected, 0);
    }

    #[test]
    fn move_selection_wraps_past_the_start() {
        let mut f = frame(3);
        f.move_selection(-1);
        assert_eq!(f.selected, 2);
    }

    #[test]
    fn move_selection_on_empty_list_does_not_panic() {
        let mut f = frame(0);
        f.move_selection(1);
        assert_eq!(f.selected, 0);
    }
}
