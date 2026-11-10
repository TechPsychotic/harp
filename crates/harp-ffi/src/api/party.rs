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

// --- your devices and shared playlists (a lasting node; see harp_party::node) ---

use crate::party_state::{node, paired, set_paired, set_shares, shares, token, LAST_MERGE, PAIRING};

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct SharedPlaylist {
    pub(crate) name: String,
    pub(crate) songs: Vec<harp_core::Song>,
}

/// One of your paired devices.
#[derive(Clone, Debug)]
pub struct DeviceView {
    pub id: String,
    pub name: String,
}

/// Starts the network side at app start when it's needed (you have paired
/// devices or shared playlists), so your other devices and friends can reach this one.
pub fn net_start_if_needed() -> Result<bool, String> {
    if paired().is_empty() && shares().is_empty() {
        return Ok(false);
    }
    node().map_err(|e| e.to_string())?;
    Ok(true)
}

/// Pairing: shows a link with a one-time code; then call `sync_wait`.
pub fn sync_offer(name: String) -> Result<String, String> {
    crate::party_state::kv_set("party_name", &name);
    let n = node().map_err(|e| e.to_string())?;
    let (link, rx) = rt().block_on(n.offer_pairing(&token()));
    *rt().block_on(PAIRING.lock()) = Some(rx);
    Ok(link)
}

/// Waits (up to 10 minutes) for the other device to use the link. Both libraries
/// are then combined, and the two devices sync by themselves from now on.
pub fn sync_wait() -> Result<super::engine::MergeInfoView, String> {
    let r = rt().block_on(async {
        let mut g = PAIRING.lock().await;
        let rx = g.as_mut().ok_or_else(|| "no pairing offer".to_string())?;
        tokio::time::timeout(std::time::Duration::from_secs(600), rx.recv())
            .await
            .map_err(|_| "no device joined in time".to_string())?
            .ok_or_else(|| "cancelled".to_string())?
            .map_err(|e| e.to_string())
    });
    *rt().block_on(PAIRING.lock()) = None;
    r?;
    Ok(LAST_MERGE.lock().unwrap().take().unwrap_or(super::engine::MergeInfoView { liked: 0, playlists_new: 0, playlists_grown: 0, plays: 0 }))
}

/// Uses another device's pairing link: combines both libraries and pairs them.
pub fn sync_join(link: String, name: String) -> Result<super::engine::MergeInfoView, String> {
    crate::party_state::kv_set("party_name", &name);
    let l = harp_party::node::Link::parse(&link).map_err(|e| e.to_string())?;
    if l.kind != "sync" {
        return Err(if l.kind == "party" { "that's a party link; this needs a sync link".into() } else { "not a sync link".into() });
    }
    let n = node().map_err(|e| e.to_string())?;
    rt().block_on(n.sync_with(l.addr(), l.params.get("code").cloned())).map_err(|e| format!("{e:#}"))?;
    Ok(LAST_MERGE.lock().unwrap().take().unwrap_or(super::engine::MergeInfoView { liked: 0, playlists_new: 0, playlists_grown: 0, plays: 0 }))
}

/// Stops a pairing offer.
pub fn sync_cancel() {
    if let Some(n) = crate::party_state::NODE.lock().unwrap().as_ref() {
        n.cancel_pairing();
    }
}

/// Your paired devices.
pub fn sync_devices() -> Vec<DeviceView> {
    paired().into_iter().map(|p| DeviceView { id: p.id, name: p.name }).collect()
}

/// Forgets a paired device (it can't sync with this one any more).
pub fn sync_unpair(id: String) {
    let mut v = paired();
    v.retain(|p| p.id != id);
    set_paired(&v);
    if let (Some(n), Ok(eid)) = (crate::party_state::NODE.lock().unwrap().as_ref(), id.parse()) {
        n.unpair(eid);
    }
}

/// Syncs with every paired device that's online now. Returns one line each.
pub fn sync_now() -> Result<Vec<String>, String> {
    let devices = paired();
    if devices.is_empty() {
        return Ok(vec![]);
    }
    let n = node().map_err(|e| e.to_string())?;
    let mut out = vec![];
    for d in devices {
        let Ok(id) = d.id.parse::<iroh::EndpointId>() else { continue };
        match rt().block_on(n.sync_with(iroh::EndpointAddr::new(id), None)) {
            Ok(_) => out.push(format!("{}: synced", d.name)),
            Err(e) => out.push(format!("{}: {}", d.name, e.to_string().lines().next().unwrap_or("not reachable"))),
        }
    }
    LAST_MERGE.lock().unwrap().take();
    Ok(out)
}

/// Shares a playlist with a lasting link: friends who save it get your changes.
pub fn playlist_share(id: i64) -> Result<String, String> {
    let mut sh = shares();
    if let Some((_, (_, link))) = sh.iter().find(|(_, (pid, _))| *pid == id) {
        return Ok(link.clone());
    }
    let (name, _) = super::engine::playlist_contents(id)?;
    let n = node().map_err(|e| e.to_string())?;
    let t = token();
    let link = rt().block_on(n.playlist_link(&t, &name));
    sh.insert(t, (id, link.clone()));
    set_shares(&sh);
    Ok(link)
}

/// The link while a playlist is shared.
#[flutter_rust_bridge::frb(sync)]
pub fn playlist_share_link(id: i64) -> Option<String> {
    shares().into_values().find(|(pid, _)| *pid == id).map(|(_, l)| l)
}

/// Stops sharing: the link stops working, friends' copies stop updating.
pub fn playlist_share_stop(id: i64) {
    let mut sh = shares();
    sh.retain(|_, (pid, _)| *pid != id);
    set_shares(&sh);
}

fn fetch_shared(link: &str) -> Result<SharedPlaylist, String> {
    let l = harp_party::node::Link::parse(link).map_err(|e| e.to_string())?;
    if l.kind != "playlist" {
        return Err("not a playlist link".into());
    }
    let n = node().map_err(|e| e.to_string())?;
    let data = rt().block_on(n.fetch_playlist(&l)).map_err(|e| format!("{e:#}"))?;
    serde_json::from_slice(&data).map_err(|_| "that playlist couldn't be read".to_string())
}

/// What a shared playlist link holds (name and songs), without saving it.
pub fn playlist_peek(link: String) -> Result<(String, Vec<SongView>), String> {
    let p = fetch_shared(&link)?;
    Ok((p.name, p.songs.iter().map(|s| view(s, false, false)).collect()))
}

/// Saves a friend's shared playlist; it follows their changes. Returns its id.
pub fn playlist_follow(link: String) -> Result<i64, String> {
    let p = fetch_shared(&link)?;
    let id = super::engine::playlist_create(p.name)?;
    super::engine::playlist_follow_set(id, &link, &p.songs)?;
    Ok(id)
}

/// Brings followed playlists up to date. Returns how many changed.
pub fn playlists_refresh() -> Result<u32, String> {
    let mut changed = 0;
    for (id, link) in super::engine::followed_playlists()? {
        if let Ok(p) = fetch_shared(&link) {
            if super::engine::playlist_follow_set(id, &link, &p.songs)? {
                changed += 1;
            }
        }
    }
    Ok(changed)
}
