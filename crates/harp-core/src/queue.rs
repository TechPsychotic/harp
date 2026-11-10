//! The queue, with the behavior people expect:
//! - tapping a result plays *that* song only ([`Queue::play_now`]);
//! - *play next* goes right after the current song;
//! - *add to queue* goes after what the user already added, before any autoplay;
//! - a playlist, album, liked songs or downloads play as the **context**
//!   ("Next from: Road trip"), after the songs you queued yourself;
//! - starting something else (a song, a mood, Smart Shuffle, another playlist)
//!   replaces the context; the songs you queued yourself stay;
//! - autoplay ("up next" / radio) is a separate section after both.

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

/// Where the music is playing from: shown in the player ("Playing from …")
/// and highlighted in the library.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// "playlist", "liked", "downloads", "album", "artist", "podcast", "local",
    /// "mood", "smart", "search", "radio", "party"
    pub kind: String,
    /// The playlist id, album id, mood key… ("" when there is none).
    pub id: String,
    /// What to show: "Road trip", "Liked Songs", "Chill"…
    pub title: String,
}

/// The sections of upcoming songs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    User,
    Context,
    Auto,
}

impl Section {
    pub fn parse(s: &str) -> Option<Section> {
        match s {
            "user" => Some(Section::User),
            "context" => Some(Section::Context),
            "auto" => Some(Section::Auto),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Queue {
    history: Vec<Song>,
    current: Option<Song>,
    user: VecDeque<Song>,
    #[serde(default)]
    context: VecDeque<Song>,
    auto: VecDeque<Song>,
    pub repeat: Repeat,
    #[serde(default)]
    pub source: Option<Source>,
    /// Saved-queue layout: 0 = before the context section existed.
    #[serde(default)]
    layout: u32,
}

/// The current saved-queue layout.
const LAYOUT: u32 = 1;

impl Queue {
    pub fn new() -> Self {
        Self { layout: LAYOUT, ..Self::default() }
    }

    /// A queue saved by an older Harp kept a playlist's songs in the same list
    /// as songs you queued yourself, so they never went away. On loading one,
    /// that list becomes the context (replaced by the next thing you start).
    pub fn upgrade(&mut self) {
        if self.layout < 1 {
            let old: Vec<Song> = self.user.drain(..).collect();
            self.context = old.into_iter().chain(self.context.drain(..)).collect();
            self.layout = LAYOUT;
        }
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
    pub fn context(&self) -> impl Iterator<Item = &Song> {
        self.context.iter()
    }
    pub fn context_len(&self) -> usize {
        self.context.len()
    }

    fn section(&mut self, s: Section) -> &mut VecDeque<Song> {
        match s {
            Section::User => &mut self.user,
            Section::Context => &mut self.context,
            Section::Auto => &mut self.auto,
        }
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

    /// Plays this song now, as something new: the songs you queued yourself
    /// stay; the old context and autoplay (chosen for what played before) go.
    pub fn play_now(&mut self, song: Song) {
        if let Some(c) = self.current.take() {
            self.history.push(c);
        }
        self.current = Some(song);
        self.context.clear();
        self.auto.clear();
        self.source = None;
    }

    /// Plays a list (playlist, album, liked songs…) as the context: its first
    /// song now, the rest after the songs you queued yourself.
    pub fn play_context(&mut self, songs: Vec<Song>, source: Option<Source>) {
        let mut it = songs.into_iter();
        let Some(first) = it.next() else { return };
        self.play_now(first);
        self.context = it.collect();
        self.source = source;
    }

    /// Smart shuffle: mixes `picks` into the playing list, one after every
    /// `every` of its songs (songs already there are skipped).
    pub fn mix_into_context(&mut self, picks: Vec<Song>, every: usize) {
        let every = every.max(1);
        let mut have: std::collections::HashSet<String> = self.context.iter().map(|s| s.id.clone()).collect();
        if let Some(c) = &self.current {
            have.insert(c.id.clone());
        }
        let mut picks = picks.into_iter().filter(|p| have.insert(p.id.clone()));
        let old: Vec<Song> = self.context.drain(..).collect();
        for (i, s) in old.into_iter().enumerate() {
            self.context.push_back(s);
            if (i + 1) % every == 0 {
                if let Some(p) = picks.next() {
                    self.context.push_back(p);
                }
            }
        }
    }

    /// Removes an upcoming song from its section.
    pub fn remove_in(&mut self, s: Section, index: usize) -> Option<Song> {
        self.section(s).remove(index)
    }

    /// Moves an upcoming song within its section (drag to reorder).
    pub fn move_in(&mut self, s: Section, from: usize, to: usize) {
        let q = self.section(s);
        if from < q.len() && to < q.len() {
            if let Some(x) = q.remove(from) {
                q.insert(to, x);
            }
        }
    }

    /// "Play next" for an upcoming song: it moves to the front of your queue;
    /// everything else keeps its order.
    pub fn promote(&mut self, s: Section, index: usize) -> bool {
        match self.section(s).remove(index) {
            Some(x) => {
                self.user.push_front(x);
                true
            }
            None => false,
        }
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
        self.remove_in(Section::User, index)
    }

    pub fn move_user(&mut self, from: usize, to: usize) {
        self.move_in(Section::User, from, to)
    }

    pub fn clear_user(&mut self) {
        self.user.clear();
    }

    /// Shuffles what is coming up (each section on its own).
    pub fn shuffle_upcoming(&mut self, rng: &mut Rng) {
        let mut u: Vec<_> = self.user.drain(..).collect();
        rng.shuffle(&mut u);
        self.user = u.into();
        let mut c: Vec<_> = self.context.drain(..).collect();
        rng.shuffle(&mut c);
        self.context = c.into();
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
        let upcoming = self.user.pop_front().or_else(|| self.context.pop_front()).or_else(|| self.auto.pop_front());
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
        self.context = all.into();
        Some(first)
    }

    /// Jumps to an item in the user's queue: the items before it are skipped
    /// (they go to history) and it starts playing.
    pub fn jump_user(&mut self, index: usize) -> Option<&Song> {
        if index >= self.user.len() {
            return None;
        }
        for _ in 0..index {
            if let Some(s) = self.user.pop_front() {
                self.history.push(s);
            }
        }
        self.next(false)
    }

    /// Jumps to an item in the context: the context songs before it are
    /// skipped (to history); your own queue stays where it is.
    pub fn jump_context(&mut self, index: usize) -> Option<&Song> {
        if index >= self.context.len() {
            return None;
        }
        for _ in 0..index {
            if let Some(s) = self.context.pop_front() {
                self.history.push(s);
            }
        }
        let s = self.context.pop_front()?;
        if let Some(c) = self.current.take() {
            self.history.push(c);
        }
        self.current = Some(s);
        self.current.as_ref()
    }

    /// Jumps to an item in the autoplay section. The user's own queue stays
    /// where it is; autoplay items before the chosen one are dropped.
    pub fn jump_auto(&mut self, index: usize) -> Option<&Song> {
        if index >= self.auto.len() {
            return None;
        }
        self.auto.drain(..index);
        let s = self.auto.pop_front()?;
        if let Some(c) = self.current.take() {
            self.history.push(c);
        }
        self.current = Some(s);
        self.current.as_ref()
    }

    pub fn previous(&mut self) -> Option<&Song> {
        let prev = self.history.pop()?;
        if let Some(c) = self.current.take() {
            self.context.push_front(c);
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
    fn jumping_skips_the_songs_in_between() {
        let mut q = Queue::new();
        q.play_now(s("cur"));
        for n in ["u1", "u2", "u3"] {
            q.add_to_queue(s(n));
        }
        q.set_autoplay(vec![s("a1"), s("a2"), s("a3")]);
        assert_eq!(q.jump_user(1).unwrap().id, "u2");
        assert_eq!(q.user_queue().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["u3"]);
        assert_eq!(q.history().len(), 2, "cur and u1 are behind us");
        assert_eq!(q.jump_auto(1).unwrap().id, "a2");
        assert_eq!(q.autoplay().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["a3"]);
        assert_eq!(q.user_len(), 1);
        assert!(q.jump_user(9).is_none());
    }

    #[test]
    fn the_whole_queue_survives_saving_and_loading() {
        let mut q = Queue::new();
        q.play_now(s("cur"));
        q.add_to_queue(s("u1"));
        q.set_autoplay(vec![s("a1")]);
        q.repeat = Repeat::All;
        q.next(false);
        let back: Queue = serde_json::from_str(&serde_json::to_string(&q).unwrap()).unwrap();
        assert_eq!(back.current().unwrap().id, "u1");
        assert_eq!(back.history().len(), 1);
        assert_eq!(back.autoplay().count(), 1);
        assert_eq!(back.repeat, Repeat::All);
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

    #[test]
    fn a_new_context_replaces_the_old_one_but_keeps_your_queue() {
        let mut q = Queue::new();
        let src = |t: &str| Some(Source { kind: "playlist".into(), id: "1".into(), title: t.into() });
        q.play_context(vec![s("p1"), s("p2"), s("p3")], src("Road"));
        q.add_to_queue(s("mine"));
        let up: Vec<_> = q.user_queue().chain(q.context()).map(|x| x.id.as_str()).collect();
        assert_eq!(up, ["mine", "p2", "p3"], "your own songs first, then the playlist");
        assert_eq!(q.source.as_ref().unwrap().title, "Road");
        // A mood / Smart Shuffle / a search result starts something new.
        q.play_now(s("mood1"));
        assert_eq!(q.context_len(), 0, "the playlist is gone");
        assert!(q.source.is_none());
        assert_eq!(q.user_queue().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["mine"]);
        // Another playlist replaces the context too.
        q.play_context(vec![s("x1"), s("x2")], src("Gym"));
        assert_eq!(q.context().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["x2"]);
    }

    #[test]
    fn promote_remove_and_reorder_in_any_section() {
        let mut q = Queue::new();
        q.play_context(vec![s("cur"), s("c1"), s("c2"), s("c3")], None);
        q.add_to_queue(s("u1"));
        q.set_autoplay(vec![s("a1"), s("a2")]);
        assert!(q.promote(Section::Context, 1), "c2 to play next");
        assert_eq!(q.user_queue().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["c2", "u1"]);
        assert_eq!(q.context().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["c1", "c3"], "the rest keep their order");
        q.promote(Section::Auto, 1);
        assert_eq!(q.user_queue().next().unwrap().id, "a2");
        assert_eq!(q.remove_in(Section::Context, 0).unwrap().id, "c1");
        q.move_in(Section::User, 0, 2);
        assert_eq!(q.user_queue().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["c2", "u1", "a2"]);
        assert!(!q.promote(Section::Auto, 5));
        let order: Vec<_> = std::iter::from_fn(|| q.next(false).map(|x| x.id.clone())).collect();
        assert_eq!(order, ["c2", "u1", "a2", "c3", "a1"]);
    }

    #[test]
    fn jumping_inside_the_context() {
        let mut q = Queue::new();
        q.play_context(vec![s("cur"), s("c1"), s("c2"), s("c3")], None);
        q.add_to_queue(s("u1"));
        assert_eq!(q.jump_context(1).unwrap().id, "c2");
        assert_eq!(q.context().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["c3"]);
        assert_eq!(q.user_len(), 1, "your queue stays");
    }

    #[test]
    fn older_saved_queues_still_load() {
        let old = r#"{"history":[],"current":null,"user":[],"auto":[],"repeat":"Off"}"#;
        let q: Queue = serde_json::from_str(old).unwrap();
        assert_eq!(q.context_len(), 0);
        assert!(q.source.is_none());
    }

    #[test]
    fn an_old_queue_full_of_playlist_songs_clears_on_the_next_start() {
        // Saved by the old Harp: a playlist's songs sitting in `user`.
        let mut old = Queue { layout: 0, ..Default::default() };
        old.current = Some(s("cur"));
        old.user = vec![s("p1"), s("p2")].into();
        let mut q: Queue = serde_json::from_str(&serde_json::to_string(&old).unwrap().replace(",\"layout\":0", "")).unwrap();
        q.upgrade();
        assert_eq!(q.user_len(), 0);
        assert_eq!(q.context().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["p1", "p2"]);
        q.play_context(vec![s("l1"), s("l2")], None);
        assert_eq!(q.context().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["l2"], "starting something new clears them");
        // A queue saved by this Harp is left alone.
        let mut new = Queue::new();
        new.add_to_queue(s("mine"));
        let mut back: Queue = serde_json::from_str(&serde_json::to_string(&new).unwrap()).unwrap();
        back.upgrade();
        assert_eq!(back.user_len(), 1, "your own queued songs stay");
    }

    #[test]
    fn smart_shuffle_mixes_picks_into_the_list() {
        let mut q = Queue::new();
        q.play_context(vec![s("cur"), s("a"), s("b"), s("c"), s("d")], None);
        q.mix_into_context(vec![s("x"), s("a"), s("y"), s("z")], 2);
        assert_eq!(q.context().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["a", "b", "x", "c", "d", "y"], "one pick after every two; duplicates skipped");
    }
}
