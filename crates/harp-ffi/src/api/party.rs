//! Listening together (M7): host a party or join one with a link. See the
//! `harp-party` crate for how it works.

use crate::frb_generated::StreamSink;
use crate::party_state::{party, rt, PARTY, SINK};
use harp_core::Song;
use harp_party::{Event, PlayState, Track};

use super::engine::{song, view, SongView};

/// The host's playback, as the app sends or receives it.
#[derive(Clone, Debug)]
pub struct PartyStateView {
    pub song: Option<SongView>,
    pub playing: bool,
    pub position_ms: u64,
}

/// Something that happened in the party. `kind`: "guest_joined", "guest_left",
/// "control", "request", "connected", "state", "members", "ended".
#[derive(Clone, Debug)]
pub struct PartyEventView {
    pub kind: String,
    /// Who (guest_joined, guest_left, control, request) or the host (connected).
    pub name: String,
    /// control: "play", "pause", "next", "previous", "seek".
    pub action: String,
    pub position_ms: u64,
    pub state: Option<PartyStateView>,
    pub members: Vec<String>,
    /// request: the suggested song.
    pub song: Option<SongView>,
    /// ended: why.
    pub reason: String,
}

fn track(s: &SongView) -> Track {
    let x = song(s);
    Track { id: x.id, title: x.title, artist: x.artist, album: x.album, thumbnail: x.thumbnail, duration_secs: x.duration_secs }
}

fn song_view(t: Track) -> SongView {
    let s = Song { id: t.id, title: t.title, artist: t.artist, artist_id: None, album: t.album, duration_secs: t.duration_secs, thumbnail: t.thumbnail };
    view(&s, false, false)
}

fn event_view(e: Event) -> PartyEventView {
    let mut v = PartyEventView { kind: String::new(), name: String::new(), action: String::new(), position_ms: 0, state: None, members: vec![], song: None, reason: String::new() };
    match e {
        Event::GuestJoined { name } => (v.kind, v.name) = ("guest_joined".into(), name),
        Event::GuestLeft { name } => (v.kind, v.name) = ("guest_left".into(), name),
        Event::Control { from, action, position_ms } => {
            (v.kind, v.name, v.action, v.position_ms) = ("control".into(), from, action, position_ms);
        }
        Event::Request { from, track } => (v.kind, v.name, v.song) = ("request".into(), from, Some(song_view(track))),
        Event::Connected { host } => (v.kind, v.name) = ("connected".into(), host),
        Event::State(s) => {
            v.kind = "state".into();
            v.state = Some(PartyStateView { song: s.track.map(song_view), playing: s.playing, position_ms: s.position_ms });
        }
        Event::Members(m) => (v.kind, v.members) = ("members".into(), m),
        Event::Ended { reason } => (v.kind, v.reason) = ("ended".into(), reason),
    }
    v
}

/// The app listens here for everything that happens in a party.
pub fn party_events(sink: StreamSink<PartyEventView>) -> Result<(), String> {
    *SINK.lock().unwrap() = Some(Box::new(move |e| {
        let _ = sink.add(event_view(e));
    }));
    Ok(())
}

/// Starts hosting; returns the invite link to share.
pub fn party_host(name: String) -> Result<String, String> {
    let p = party(&name).map_err(|e| e.to_string())?;
    rt().block_on(p.host()).map_err(|e| e.to_string())
}

/// Joins a party from its invite link.
pub fn party_join(link: String, name: String) -> Result<(), String> {
    let p = party(&name).map_err(|e| e.to_string())?;
    rt().block_on(p.join(&link)).map_err(|e| format!("{e:#}"))
}

/// Leaves the party (or ends it, for the host).
pub fn party_leave() -> Result<(), String> {
    if let Some(p) = PARTY.lock().unwrap().clone() {
        rt().block_on(p.leave());
    }
    Ok(())
}

/// Host: our player's state, for the guests.
pub fn party_publish(state: PartyStateView) -> Result<(), String> {
    if let Some(p) = PARTY.lock().unwrap().clone() {
        p.publish(PlayState { track: state.song.as_ref().map(track), playing: state.playing, position_ms: state.position_ms });
    }
    Ok(())
}

/// Guest: play, pause, next, previous, or seek.
pub fn party_control(action: String, position_ms: u64) -> Result<(), String> {
    let p = PARTY.lock().unwrap().clone().ok_or("not in a party")?;
    p.control(&action, position_ms).map_err(|e| e.to_string())
}

/// Guest: suggest a song to the host.
pub fn party_request(s: SongView) -> Result<(), String> {
    let p = PARTY.lock().unwrap().clone().ok_or("not in a party")?;
    p.request(track(&s)).map_err(|e| e.to_string())
}

/// "none", "host" or "guest".
#[flutter_rust_bridge::frb(sync)]
pub fn party_role() -> String {
    match PARTY.lock().unwrap().as_ref() {
        Some(p) if p.is_hosting() => "host".into(),
        Some(p) if p.is_guest() => "guest".into(),
        _ => "none".into(),
    }
}

// --- syncing your library between your own devices ---

/// Offers this device's library for syncing; returns the link to paste on the
/// other device. Then call `sync_wait`.
pub fn sync_offer(name: String) -> Result<String, String> {
    let ours = super::engine::library_backup()?;
    let offer = rt().block_on(harp_party::sync::Offer::start(ours, &name, false)).map_err(|e| e.to_string())?;
    let link = offer.link.clone();
    *crate::party_state::OFFER.lock().unwrap() = Some(std::sync::Arc::new(offer));
    Ok(link)
}

/// Waits (up to 10 minutes) for the other device, then adds its library to this one.
pub fn sync_wait() -> Result<super::engine::MergeInfoView, String> {
    let offer = crate::party_state::OFFER.lock().unwrap().clone().ok_or("no sync offer")?;
    let theirs = rt().block_on(offer.wait(std::time::Duration::from_secs(600))).map_err(|e| e.to_string());
    crate::party_state::OFFER.lock().unwrap().take();
    if let Ok(o) = std::sync::Arc::try_unwrap(offer) {
        rt().spawn(o.close());
    }
    super::engine::library_restore(theirs?)
}

/// Joins a sync offer: both devices end up with both libraries.
pub fn sync_join(link: String) -> Result<super::engine::MergeInfoView, String> {
    let ours = super::engine::library_backup()?;
    let theirs = rt().block_on(harp_party::sync::join(&link, &ours)).map_err(|e| format!("{e:#}"))?;
    super::engine::library_restore(theirs)
}

/// Stops offering (a `sync_wait` in progress returns "cancelled" and closes it).
pub fn sync_cancel() {
    if let Some(o) = crate::party_state::OFFER.lock().unwrap().as_ref() {
        o.cancel();
    }
}
