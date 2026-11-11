//! CedarApple's typed view of Lumtub's flat YouTube listings.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";
// lib/youtube_web.dart and lib/youtube_listing.dart in Lumtub 0.2.36.
const CLIENT_VERSION: &str = "2.20260708.00.00";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub channel: String,
    pub channel_id: String,
    pub description: String,
    pub duration: u64,
    pub views: u64,
    pub live: bool,
    pub members_only: bool,
    pub thumbnail: String,
}

pub fn video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

impl Item {
    pub fn from_entry(v: &Value) -> Option<Self> {
        let s = |key| v.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
        let id = s("id");
        let kind = match v.get("ie_key")?.as_str()? {
            "Youtube" if video_id(&id) => "video",
            "YoutubeTab" if id.starts_with("UC") => "channel",
            "YoutubeTab" | "YoutubePlaylist" => "playlist",
            _ => return None,
        };
        if id.is_empty() {
            return None;
        }
        let thumb = v
            .get("thumbnails")
            .and_then(Value::as_array)
            .and_then(|list| list.last())
            .and_then(|t| t.get("url"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| {
                if kind == "video" {
                    format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg")
                } else {
                    String::new()
                }
            });
        Some(Self {
            id,
            title: s("title"),
            kind: kind.into(),
            channel: s("channel"),
            channel_id: s("channel_id"),
            description: s("description"),
            duration: v.get("duration").and_then(Value::as_u64).unwrap_or(0),
            views: v
                .get("view_count")
                .or_else(|| v.get("concurrent_view_count"))
                .and_then(Value::as_u64)
                .unwrap_or(0),
            live: s("live_status") == "is_live",
            members_only: s("availability") == "subscriber_only",
            thumbnail: if thumb.starts_with("//") {
                format!("https:{thumb}")
            } else {
                thumb
            },
        })
    }

    pub fn detail(&self) -> String {
        if self.kind == "channel" {
            return "Channel · Open to browse videos".into();
        }
        if self.kind == "playlist" {
            return "Playlist · Open to browse videos".into();
        }
        let mut details = Vec::new();
        if !self.channel.is_empty() {
            details.push(self.channel.clone());
        }
        if self.live {
            details.push("LIVE".into());
        } else if self.duration > 0 {
            let n = self.duration;
            details.push(if n >= 3600 {
                format!("{}:{:02}:{:02}", n / 3600, n / 60 % 60, n % 60)
            } else {
                format!("{}:{:02}", n / 60, n % 60)
            });
        }
        if self.views > 0 {
            details.push(format!("{} views", self.views));
        }
        if self.members_only {
            details.push("Members only".into());
        }
        details.join(" · ")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Search(String, String),
    Channel(String, String),
    Playlist(String),
    Shelf(String, String),
    Saved,
    History,
    Channels,
}

impl Page {
    pub fn request(&self) -> Option<(&'static str, Value)> {
        Some(match self {
            Self::Search(query, filter) => {
                let params = match filter.as_str() {
                    "Videos" => "EgIQAQ%3D%3D",
                    "Shorts" => "EgIQCQ%3D%3D",
                    "Live" => "EgJAAQ%3D%3D",
                    "Channels" => "EgIQAg%3D%3D",
                    "Playlists" => "EgIQAw%3D%3D",
                    _ => "",
                };
                ("search", json!({"query": query, "params": params}))
            }
            Self::Channel(id, tab) => (
                "browse",
                json!({"browseId": id, "params": match tab.as_str() {
                    "Shorts" => "EgZzaG9ydHPyBgUKA5oBAA%3D%3D",
                    "Live" => "EgdzdHJlYW1z8gYECgJ6AA%3D%3D",
                    "Playlists" => "EglwbGF5bGlzdHPyBgQKAkIA", _ => "EgZ2aWRlb3PyBgQKAjoA",
                }}),
            ),
            Self::Playlist(id) => ("browse", json!({"browseId": format!("VL{id}")})),
            Self::Shelf(id, params) => ("browse", json!({"browseId": id, "params": params})),
            _ => return None,
        })
    }
}

pub async fn fetch(
    page: &Page,
    continuation: Option<&str>,
) -> Result<super::listing::Page, String> {
    let (endpoint, mut body) = page.request().ok_or("This page is stored locally")?;
    if let Some(token) = continuation {
        body = json!({"continuation": token});
    }
    body["context"] = json!({"client": {
        "clientName": "WEB", "clientVersion": CLIENT_VERSION, "hl": "en",
        "timeZone": "UTC", "utcOffsetMinutes": 0,
    }});
    let result: Value = reqwest::Client::new()
        .post(format!(
            "https://www.youtube.com/youtubei/v1/{endpoint}?prettyPrint=false"
        ))
        .timeout(std::time::Duration::from_secs(25))
        .header("User-Agent", USER_AGENT)
        .header("Accept-Language", "en")
        .header("Origin", "https://www.youtube.com")
        .header("X-YouTube-Client-Name", "1")
        .header("X-YouTube-Client-Version", CLIENT_VERSION)
        .header("Cookie", "CONSENT=YES+cb; SOCS=CAI")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Could not reach YouTube: {e}"))?
        .error_for_status()
        .map_err(|e| format!("YouTube refused this request: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Could not read YouTube's response: {e}"))?;
    if let Some(error) = result.get("error") {
        return Err(format!("YouTube: {error}"));
    }
    Ok(super::listing::page(&result))
}

/// Accept only YouTube addresses, and never pass an arbitrary URL to yt-dlp.
pub fn address(text: &str) -> Option<PageOrVideo> {
    let url = url::Url::parse(text).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?;
    let parts: Vec<_> = url.path_segments()?.collect();
    let id = if host == "youtu.be" {
        parts.first().copied().map(str::to_owned)
    } else if matches!(
        host,
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com"
    ) {
        if parts.first() == Some(&"watch") {
            url.query_pairs()
                .find(|(k, _)| k == "v")
                .map(|(_, v)| v.into_owned())
        } else if matches!(parts.first(), Some(&"shorts" | &"live" | &"embed")) {
            parts.get(1).map(|s| (*s).to_owned())
        } else {
            if parts.first() == Some(&"channel")
                && let Some(id) = parts.get(1).filter(|s| s.starts_with("UC"))
            {
                return Some(PageOrVideo::Page(Page::Channel(
                    (*id).into(),
                    "Videos".into(),
                )));
            }
            let list = url
                .query_pairs()
                .find(|(k, _)| k == "list")
                .map(|(_, v)| v.into_owned())?;
            return Some(PageOrVideo::Page(Page::Playlist(list)));
        }
    } else {
        return None;
    }?;
    video_id(&id).then_some(PageOrVideo::Video(id))
}

pub enum PageOrVideo {
    Page(Page),
    Video(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_youtube_addresses_without_accepting_other_hosts() {
        assert!(
            matches!(address("https://youtu.be/abcdefghijk?t=10"), Some(PageOrVideo::Video(id)) if id == "abcdefghijk")
        );
        assert!(matches!(
            address("https://youtube.com/shorts/abcdefghijk"),
            Some(PageOrVideo::Video(_))
        ));
        assert!(matches!(
            address("https://youtube.com/playlist?list=PL123"),
            Some(PageOrVideo::Page(Page::Playlist(_)))
        ));
        assert!(address("https://youtube.com.evil.test/watch?v=abcdefghijk").is_none());
        assert!(address("https://youtu.be/--exec").is_none());
    }
    #[test]
    fn creates_paged_requests_for_channels_and_filters() {
        let (endpoint, body) = Page::Search("birds".into(), "Channels".into())
            .request()
            .unwrap();
        assert_eq!(endpoint, "search");
        assert_eq!(body["params"], "EgIQAg%3D%3D");
        let (_, body) = Page::Playlist("PL123".into()).request().unwrap();
        assert_eq!(body["browseId"], "VLPL123");
    }

    #[tokio::test]
    #[ignore = "contacts YouTube; run explicitly to check live browsing"]
    async fn live_search_smoke() {
        let page = fetch(
            &Page::Search("Blender Big Buck Bunny".into(), "Videos".into()),
            None,
        )
        .await
        .unwrap();
        let items: Vec<_> = page.entries.iter().filter_map(Item::from_entry).collect();
        assert!(
            items.iter().any(|i| i.kind == "video"),
            "YouTube returned no videos"
        );
        assert!(
            page.continuation.is_some(),
            "YouTube did not offer another page"
        );
    }
}
