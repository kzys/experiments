// A thin client for the slice of the Jellyfin REST API this player needs:
// authenticate, browse the artist/album/track hierarchy, and build a stream
// URL. See https://api.jellyfin.org for the full surface.

use serde::Deserialize;

use crate::util::die;

// Identifies this client to the server; doesn't need to be unique per
// install since Jellyfin only uses it for display (Dashboard > Devices).
const DEVICE_ID: &str = "rg35xx-music";

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

#[derive(Deserialize, Clone)]
pub struct Item {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Name")]
    pub name: String,
}

// Authenticates with a username/password and returns a client bound to the
// resulting user and access token. Panics (via die) on any failure: this is
// step one of the program, and there's nothing useful to do without it.
pub fn authenticate(server: &str, username: &str, password: &str) -> Client {
    let agent = ureq::Agent::new();
    let auth_header = format!(
        r#"MediaBrowser Client="music", Device="RG35xx", DeviceId="{DEVICE_ID}", Version="0.1.0""#
    );

    let url = format!("{server}/Users/AuthenticateByName");
    let resp = agent
        .post(&url)
        .set("X-Emby-Authorization", &auth_header)
        .send_json(ureq::json!({ "Username": username, "Pw": password }));

    let resp = match resp {
        Ok(r) => r,
        Err(e) => die(&format!("authentication failed: {e}")),
    };
    let auth: AuthResponse = resp.into_json().unwrap_or_else(|e| die(&format!("bad auth response: {e}")));

    Client { agent, server: server.to_string(), token: auth.access_token, user_id: auth.user.id }
}

impl Client {
    fn get_items(&self, query: &[(&str, &str)]) -> Vec<Item> {
        let url = format!("{}/Users/{}/Items", self.server, self.user_id);
        let mut req = self.agent.get(&url).set("X-Emby-Token", &self.token);
        for (k, v) in query {
            req = req.query(k, v);
        }
        let resp = req.call().unwrap_or_else(|e| die(&format!("listing items failed: {e}")));
        let items: ItemsResponse =
            resp.into_json().unwrap_or_else(|e| die(&format!("bad items response: {e}")));
        items.items
    }

    // No paging on any of these: fine for a home library, not for a huge one.

    pub fn list_artists(&self) -> Vec<Item> {
        self.get_items(&[("IncludeItemTypes", "MusicArtist"), ("Recursive", "true"), ("SortBy", "SortName")])
    }

    // ArtistIds (a tag-based relationship), not ParentId (folder-based): a
    // library's on-disk layout doesn't reliably nest each artist's albums
    // under one folder, but Jellyfin's own artist/album tagging does.
    pub fn list_albums(&self, artist_id: &str) -> Vec<Item> {
        self.get_items(&[
            ("ArtistIds", artist_id),
            ("IncludeItemTypes", "MusicAlbum"),
            ("Recursive", "true"),
            ("SortBy", "SortName"),
        ])
    }

    // ParentId here, unlike artists->albums above: an album's tracks really
    // do live in one folder, so the folder hierarchy is the right query.
    pub fn list_tracks(&self, album_id: &str) -> Vec<Item> {
        self.get_items(&[("ParentId", album_id), ("IncludeItemTypes", "Audio"), ("SortBy", "IndexNumber")])
    }

    // Direct-play URL for an item; the server transcodes only if the
    // original format needs it (static=true asks it not to bother otherwise).
    pub fn stream_url(&self, item_id: &str) -> String {
        format!("{}/Audio/{}/stream?static=true&api_key={}", self.server, item_id, self.token)
    }
}
