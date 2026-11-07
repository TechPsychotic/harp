//! The queue, with the behavior people expect:
//! - tapping a result plays *that* song only ([`Queue::play_now`]);
//! - *play next* goes right after the current song;
//! - *add to queue* goes after what the user already added, before any autoplay;
//! - autoplay ("up next" / radio) is a separate section after the user's queue.

use crate::model::Song;
use crate::rng::Rng;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Queue {
    history: Vec<Song>,
    current: Option<Song>,
    user: VecDeque<Song>,
    auto: VecDeque<Song>,
    pub repeat: Repeat,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> Option<&Song> {
        self.current.as_ref()
    }
    pub fn user_queue(&self) -> impl Iterator<Item = &Song> {
        self.user.iter()
    }
    pub fn autoplay(&self) -> impl Iterator<Item = &Song> {
        self.auto.iter()
    }
    pub fn history(&self) -> &[Song] {
        &self.history
    }
    pub fn user_len(&self) -> usize {
        self.user.len()
    }
    pub fn auto_len(&self) -> usize {
        self.auto.len()
    }

    /// Plays this song now. The user's queue stays; autoplay is discarded
    /// because it was chosen for the previous song.
    pub fn play_now(&mut self, song: Song) {
        if let Some(c) = self.current.take() {
            self.history.push(c);
        }
        self.current = Some(song);
        self.auto.clear();
    }

    pub fn play_next(&mut self, song: Song) {
        self.user.push_front(song);
    }

    pub fn add_to_queue(&mut self, song: Song) {
        self.user.push_back(song);
    }

    pub fn set_autoplay(&mut self, songs: Vec<Song>) {
        self.auto = songs.into();
    }

    pub fn remove_user(&mut self, index: usize) -> Option<Song> {
        self.user.remove(index)
    }

    pub fn move_user(&mut self, from: usize, to: usize) {
        if from < self.user.len() && to < self.user.len() {
            if let Some(s) = self.user.remove(from) {
                self.user.insert(to, s);
            }
        }
    }

    pub fn clear_user(&mut self) {
        self.user.clear();
    }

    /// Shuffles what is coming up (user's queue and autoplay separately).
    pub fn shuffle_upcoming(&mut self, rng: &mut Rng) {
        let mut u: Vec<_> = self.user.drain(..).collect();
        rng.shuffle(&mut u);
        self.user = u.into();
        let mut a: Vec<_> = self.auto.drain(..).collect();
        rng.shuffle(&mut a);
        self.auto = a.into();
    }

    /// Advances to the next song. `None` when nothing is left (and repeat is off).
    /// `auto_advance` is true when the song ended on its own: Repeat::One then
    /// replays it, whereas a manual skip moves on.
    pub fn next(&mut self, auto_advance: bool) -> Option<&Song> {
        if auto_advance && self.repeat == Repeat::One && self.current.is_some() {
            return self.current.as_ref();
        }
        let upcoming = self.user.pop_front().or_else(|| self.auto.pop_front());
        let upcoming = match upcoming {
            Some(s) => Some(s),
            None if self.repeat != Repeat::Off => self.restart_from_history(),
            None => None,
        };
        let s = upcoming?;
        if let Some(c) = self.current.take() {
            self.history.push(c);
        }
        self.current = Some(s);
        self.current.as_ref()
    }

    /// Repeat-all: when everything has played, start over from the first song played.
    fn restart_from_history(&mut self) -> Option<Song> {
        let mut all: Vec<Song> = std::mem::take(&mut self.history);
        if let Some(c) = self.current.take() {
            all.push(c);
        }
        if all.is_empty() {
            return None;
        }
        let first = all.remove(0);
        self.user = all.into();
        Some(first)
    }

    pub fn previous(&mut self) -> Option<&Song> {
        let prev = self.history.pop()?;
        if let Some(c) = self.current.take() {
            self.user.push_front(c);
        }
        self.current = Some(prev);
        self.current.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::song;

    fn s(n: &str) -> Song {
        song(n, n, "a")
    }
    fn id(q: &Queue) -> Option<&str> {
        q.current().map(|c| c.id.as_str())
    }

    #[test]
    fn play_now_plays_only_that_song() {
        let mut q = Queue::new();
        q.play_now(s("a"));
        assert_eq!(id(&q), Some("a"));
        assert!(q.next(false).is_none());
    }

    #[test]
    fn play_next_and_add_ordering_with_autoplay_last() {
        let mut q = Queue::new();
        q.play_now(s("cur"));
        q.set_autoplay(vec![s("auto1"), s("auto2")]);
        q.add_to_queue(s("u1"));
        q.add_to_queue(s("u2"));
        q.play_next(s("n"));
        let order: Vec<_> = std::iter::from_fn(|| q.next(false).map(|x| x.id.clone())).collect();
        assert_eq!(order, ["n", "u1", "u2", "auto1", "auto2"]);
    }

    #[test]
    fn play_now_drops_stale_autoplay_keeps_user_queue() {
        let mut q = Queue::new();
        q.play_now(s("a"));
        q.set_autoplay(vec![s("x")]);
        q.add_to_queue(s("u"));
        q.play_now(s("b"));
        assert_eq!(q.auto_len(), 0);
        assert_eq!(q.user_len(), 1);
    }

    #[test]
    fn repeat_one_only_on_auto_advance() {
        let mut q = Queue::new();
        q.play_now(s("a"));
        q.add_to_queue(s("b"));
        q.repeat = Repeat::One;
        assert_eq!(q.next(true).map(|x| x.id.as_str()), Some("a"));
        assert_eq!(q.next(false).map(|x| x.id.as_str()), Some("b"));
    }

    #[test]
    fn repeat_all_wraps() {
        let mut q = Queue::new();
        q.play_now(s("a"));
        q.add_to_queue(s("b"));
        q.repeat = Repeat::All;
        assert_eq!(q.next(true).unwrap().id, "b");
        assert_eq!(q.next(true).unwrap().id, "a");
        assert_eq!(q.next(true).unwrap().id, "b");
    }

    #[test]
    fn previous_returns_and_requeues_current() {
        let mut q = Queue::new();
        q.play_now(s("a"));
        q.add_to_queue(s("b"));
        q.next(false);
        assert_eq!(q.previous().unwrap().id, "a");
        assert_eq!(q.next(false).unwrap().id, "b");
    }

    #[test]
    fn move_and_remove() {
        let mut q = Queue::new();
        for n in ["a", "b", "c"] {
            q.add_to_queue(s(n));
        }
        q.move_user(2, 0);
        assert_eq!(q.user_queue().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["c", "a", "b"]);
        assert_eq!(q.remove_user(1).unwrap().id, "a");
    }
}
