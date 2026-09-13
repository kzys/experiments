// A thin client for the slice of the Jellyfin REST API this player needs:
// authenticate, browse the album/track hierarchy, and build a stream URL.
// See https://api.jellyfin.org for the full surface.

use std::time::Duration;

use serde::Deserialize;

// Identifies this client to the server; doesn't need to be unique per
// install since Jellyfin only uses it for display (Dashboard > Devices).
const DEVICE_ID: &str = "rg35xx-music";

// A handheld on WiFi has a flaky enough connection that a hung request is a
// real case, not a hypothetical: without a bound, ureq's default is to wait
// on the OS's own TCP connect timeout (minutes), leaving the kiosk screen
// blank that whole time. Every request gets this same bound (see
// authenticate's AgentBuilder).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Client {
    agent: ureq::Agent,
    server: String,
    token: String,
    pub user_id: String,
}

#[derive(Deserialize)]
struct AuthResponse {
    #[serde(rename = "AccessToken")]
    access_token: String,
    #[serde(rename = "User")]
    user: AuthUser,
}

#[derive(Deserialize)]
struct AuthUser {
    #[serde(rename = "Id")]
    id: String,
}

#[derive(Deserialize)]
struct ItemsResponse {
    #[serde(rename = "Items")]
    items: Vec<Item>,
}

#[derive(Deserialize, Clone, Default)]
pub struct Item {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Name")]
    pub name: String,
    // Track number within its album; absent on albums and untagged tracks.
    #[serde(rename = "IndexNumber")]
    pub index_number: Option<u32>,
    // Duration in 100ns ticks (Jellyfin's unit); absent on albums.
    #[serde(rename = "RunTimeTicks")]
    pub run_time_ticks: Option<u64>,
    // Number of tracks; albums only, and only when the listing asks for it
    // (see list_albums).
    #[serde(rename = "ChildCount")]
    pub child_count: Option<u32>,
}

impl Item {
    pub fn duration_secs(&self) -> Option<u64> {
        self.run_time_ticks.map(|t| t / 10_000_000)
    }
}

// Authenticates with a username/password and returns a client bound to the
// resulting user and access token. Returns the failure as a message rather
// than dying outright: a flaky connection makes this a routine failure the
// UI should show, not a startup-only panic.
pub fn authenticate(server: &str, username: &str, password: &str) -> Result<Client, String> {
    let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
    let auth_header = format!(
        r#"MediaBrowser Client="music", Device="RG35xx", DeviceId="{DEVICE_ID}", Version="0.1.0""#
    );

    let url = format!("{server}/Users/AuthenticateByName");
    let resp = agent
        .post(&url)
        .set("X-Emby-Authorization", &auth_header)
        .send_json(ureq::json!({ "Username": username, "Pw": password }))
        .map_err(|e| format!("authentication failed: {e}"))?;
    let auth: AuthResponse = resp.into_json().map_err(|e| format!("bad auth response: {e}"))?;

    Ok(Client { agent, server: server.to_string(), token: auth.access_token, user_id: auth.user.id })
}

impl Client {
    fn get_items(&self, query: &[(&str, &str)]) -> Result<Vec<Item>, String> {
        let url = format!("{}/Users/{}/Items", self.server, self.user_id);
        let mut req = self.agent.get(&url).set("X-Emby-Token", &self.token);
        for (k, v) in query {
            req = req.query(k, v);
        }
        let resp = req.call().map_err(|e| format!("listing items failed: {e}"))?;
        let items: ItemsResponse = resp.into_json().map_err(|e| format!("bad items response: {e}"))?;
        Ok(items.items)
    }

    // No paging on any of these: fine for a home library, not for a huge one.

    // Deliberately not artist -> album: Jellyfin's artist tagging on this
    // library is a mess ("feat. X" splitting one act into several distinct
    // MusicArtist entries, "VA" compilations, etc), and most artists here
    // are tag-only ("virtual", no folder of their own) rather than a node
    // in the physical folder tree, which a Recursive Items walk can't see
    // at all — confirmed IncludeItemTypes=MusicArtist here finds 3 of the
    // library's real 238 artists. Albums don't have that problem: they're
    // real folders, so a flat recursive listing gets all of them regardless
    // of how tangled the artist tagging above them is.
    pub fn list_albums(&self) -> Result<Vec<Item>, String> {
        self.get_items(&[
            ("IncludeItemTypes", "MusicAlbum"),
            ("Recursive", "true"),
            ("SortBy", "SortName"),
            ("Fields", "ChildCount"),
        ])
    }

    // ParentId (folder-based): an album's tracks really do live in one
    // folder, so the folder hierarchy is the right query here.
    pub fn list_tracks(&self, album_id: &str) -> Result<Vec<Item>, String> {
        self.get_items(&[("ParentId", album_id), ("IncludeItemTypes", "Audio"), ("SortBy", "IndexNumber")])
    }

    // Direct-play URL for an item; the server transcodes only if the
    // original format needs it (static=true asks it not to bother otherwise).
    pub fn stream_url(&self, item_id: &str) -> String {
        format!("{}/Audio/{}/stream?static=true&api_key={}", self.server, item_id, self.token)
    }
}
