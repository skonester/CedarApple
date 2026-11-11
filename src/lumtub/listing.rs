// Ported from Lumtub 0.2.36, GPL-3.0-or-later; see licenses/Lumtub.txt.
//! Search results and channel tabs straight from YouTube's web API, read
//! into the same flat entries `yt-dlp --flat-playlist --dump-json` prints,
//! so Lumtub need not start yt-dlp to list them. The renderers and paths
//! follow yt-dlp's extractor/youtube/_tab.py; responses are asked for in
//! English, as yt-dlp asks, so counts and durations read the same way.

use serde::Serialize;
use serde_json::{Value, json};

use super::innertube::{at, members_only, seconds};

/// `simpleText`, or the joined `runs` of [node].
fn get_text(node: Option<&Value>) -> Option<String> {
    let node = node?;
    if let Some(Value::String(simple)) = node.get("simpleText") {
        return Some(simple.clone()).filter(|s| !s.is_empty());
    }
    let runs = node.get("runs")?.as_array()?;
    let text: String = runs
        .iter()
        .filter_map(|r| r.get("text")?.as_str())
        .collect();
    (!text.is_empty()).then_some(text)
}

/// The first of [keys] of [node] that has text.
fn text_of(node: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| get_text(node.get(*key)))
}

/// Counts such as `1,234 views`, `1.2M subscribers` or `No views`.
/// How long ago YouTube's English time text says, in seconds: "3 days
/// ago", "Streamed 2 weeks ago", "1.2M views • 15 years ago".
pub fn ago(text: &str) -> Option<u64> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let at = words.iter().position(|w| w.eq_ignore_ascii_case("ago"))?;
    let (number, unit) = (
        words.get(at.checked_sub(2)?)?,
        words.get(at - 1)?.to_ascii_lowercase(),
    );
    let n: u64 = number.parse().ok()?;
    let unit = match unit.trim_end_matches('s') {
        "second" | "sec" => 1,
        "minute" | "min" => 60,
        "hour" | "hr" => 3_600,
        "day" => 86_400,
        "week" | "wk" => 7 * 86_400,
        "month" | "mo" => 30 * 86_400,
        "year" | "yr" => 365 * 86_400,
        _ => return None,
    };
    n.checked_mul(unit)
}

pub fn count(text: &str) -> Option<u64> {
    let text = text.trim();
    if text
        .as_bytes()
        .get(..3)
        .is_some_and(|p| p.eq_ignore_ascii_case(b"no "))
    {
        return Some(0);
    }
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(rest.len());
    let number = &rest[..end];
    let after = rest[end..].trim_start();
    let unit = after
        .split(|c: char| !c.is_ascii_alphabetic())
        .next()
        .unwrap_or("");
    let factor = match unit {
        "K" | "k" => 1e3,
        "M" | "m" | "KK" | "kk" => 1e6,
        "B" | "b" => 1e9,
        _ => {
            // A plain number: its digits, as yt-dlp's str_to_int reads it.
            let digits: String = number.chars().filter(char::is_ascii_digit).collect();
            return digits.parse().ok();
        }
    };
    let value: f64 = number.replace(',', "").parse().ok()?;
    Some((value * factor).round() as u64)
}

fn is_channel_id(id: &str) -> bool {
    id.len() == 24
        && id.starts_with("UC")
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The `@handle` at the start of a channel address.
fn handle_from_url(url: &str) -> Option<String> {
    let path = url
        .strip_prefix("https://www.youtube.com")
        .or_else(|| url.strip_prefix("http://www.youtube.com"))
        .or_else(|| url.strip_prefix("https://youtube.com"))
        .unwrap_or(url);
    let handle = path.strip_prefix('/')?;
    let end = handle.find(['/', '?', '#']).unwrap_or(handle.len());
    let handle = &handle[..end];
    (handle.starts_with('@') && handle.len() > 1).then(|| handle.to_owned())
}

fn thumbnails(node: Option<&Value>, key: &str) -> Value {
    let list = node
        .and_then(|n| n.get(key))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    Value::Array(
        list.iter()
            .filter_map(|t| t.get("url")?.as_str())
            .map(|url| json!({"url": url}))
            .collect(),
    )
}

fn browse_ids(runs: Option<&Value>) -> impl Iterator<Item = &Value> {
    runs.and_then(|r| r.get("runs"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|run| run.get("navigationEndpoint"))
}

/// An entry with the null fields left out.
fn entry(fields: Vec<(&str, Value)>) -> Value {
    Value::Object(
        fields
            .into_iter()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
}

fn opt<T: Into<Value>>(value: Option<T>) -> Value {
    value.map_or(Value::Null, Into::into)
}

/// yt-dlp's `_extract_video`, for `videoRenderer` and its relatives.
fn video(renderer: &Value) -> Option<Value> {
    let id = renderer.get("videoId")?.as_str()?;
    let title = text_of(renderer, &["title", "headline"]);
    let description = text_of(renderer, &["descriptionSnippet"]).or_else(|| {
        renderer
            .get("detailedMetadataSnippets")?
            .as_array()?
            .iter()
            .find_map(|s| get_text(s.get("snippetText")))
    });
    let overlays = renderer
        .get("thumbnailOverlays")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let time_status = overlays
        .iter()
        .find_map(|o| o.get("thumbnailOverlayTimeStatusRenderer"));
    let duration = renderer
        .get("lengthSeconds")
        .and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse().ok())
                .or_else(|| v.as_u64().map(|n| n as u32))
        })
        .or_else(|| text_of(renderer, &["lengthText"]).and_then(|t| seconds(&t)))
        .or_else(|| get_text(time_status.and_then(|t| t.get("text"))).and_then(|t| seconds(&t)));
    let channel_id = browse_ids(renderer.get("shortBylineText"))
        .find_map(|e| at(e, &["browseEndpoint", "browseId"])?.as_str())
        .filter(|id| is_channel_id(id));
    let overlay_style = overlays
        .iter()
        .find_map(|o| at(o, &["thumbnailOverlayTimeStatusRenderer", "style"])?.as_str());
    let badges: Vec<&str> = renderer
        .get("badges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|b| at(b, &["metadataBadgeRenderer", "style"])?.as_str())
        .collect();
    let navigation = at(
        renderer,
        &[
            "navigationEndpoint",
            "commandMetadata",
            "webCommandMetadata",
            "url",
        ],
    )
    .and_then(Value::as_str)
    .unwrap_or("");
    let url = if overlay_style == Some("SHORTS") || navigation.contains("/shorts/") {
        format!("https://www.youtube.com/shorts/{id}")
    } else {
        format!("https://www.youtube.com/watch?v={id}")
    };
    let time_text = text_of(renderer, &["publishedTimeText", "videoInfo"]).unwrap_or_default();
    let scheduled = at(renderer, &["upcomingEventData", "startTime"]).and_then(|v| {
        v.as_str()
            .and_then(|s| s.parse::<i64>().ok())
            .or_else(|| v.as_i64())
    });
    let live_status = if scheduled.is_some() {
        Some("is_upcoming")
    } else if time_text.to_lowercase().contains("streamed") {
        Some("was_live")
    } else if overlay_style == Some("LIVE") || badges.contains(&"BADGE_STYLE_TYPE_LIVE_NOW") {
        Some("is_live")
    } else {
        None
    };
    let views_text = text_of(
        renderer,
        &["viewCountText", "shortViewCountText", "videoInfo"],
    )
    .unwrap_or_default();
    let views = count(&views_text);
    let live = matches!(live_status, Some("is_live" | "is_upcoming"));
    // About when it was published, as yt-dlp's approximate_date gives it.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let published = if live {
        None
    } else {
        ago(&time_text).and_then(|seconds| now.checked_sub(seconds))
    };
    let channel = text_of(renderer, &["ownerText", "shortBylineText"]);
    let handle = browse_ids(renderer.get("shortBylineText")).find_map(|e| {
        at(e, &["commandMetadata", "webCommandMetadata", "url"])
            .or_else(|| at(e, &["browseEndpoint", "canonicalBaseUrl"]))?
            .as_str()
            .and_then(handle_from_url)
    });
    Some(entry(vec![
        ("_type", "url".into()),
        ("ie_key", "Youtube".into()),
        ("id", id.into()),
        ("url", url.into()),
        ("title", opt(title)),
        ("description", opt(description)),
        ("duration", opt(duration)),
        ("channel_id", opt(channel_id)),
        ("channel", opt(channel.clone())),
        ("uploader", opt(channel)),
        ("uploader_id", opt(handle)),
        ("release_timestamp", opt(scheduled)),
        ("timestamp", opt(published)),
        (
            if live {
                "concurrent_view_count"
            } else {
                "view_count"
            },
            opt(views),
        ),
        ("live_status", opt(live_status)),
        (
            "availability",
            opt(members_only(renderer).then_some("subscriber_only")),
        ),
    ]))
}

/// yt-dlp's `_extract_channel_renderer`.
fn channel(renderer: &Value) -> Option<Value> {
    let id = renderer
        .get("channelId")?
        .as_str()
        .filter(|id| is_channel_id(id))?;
    let title = text_of(renderer, &["title"]);
    let handle = at(
        renderer,
        &[
            "navigationEndpoint",
            "commandMetadata",
            "webCommandMetadata",
            "url",
        ],
    )
    .or_else(|| {
        at(
            renderer,
            &["navigationEndpoint", "browseEndpoint", "canonicalBaseUrl"],
        )
    })
    .and_then(Value::as_str)
    .and_then(handle_from_url)
    // Search results carry the handle in subscriberCountText.
    .or_else(|| {
        text_of(renderer, &["subscriberCountText"])
            .filter(|t| t.starts_with('@') && !t.contains(char::is_whitespace))
    });
    let followers = text_of(renderer, &["subscriberCountText"])
        .and_then(|t| count(&t))
        .or_else(|| text_of(renderer, &["videoCountText"]).and_then(|t| count(&t)));
    let url = format!("https://www.youtube.com/channel/{id}");
    Some(entry(vec![
        ("_type", "url".into()),
        ("ie_key", "YoutubeTab".into()),
        ("url", url.clone().into()),
        ("id", id.into()),
        ("channel", opt(title.clone())),
        ("uploader", opt(title.clone())),
        ("channel_id", id.into()),
        ("channel_url", url.into()),
        ("title", opt(title)),
        ("uploader_id", opt(handle)),
        ("channel_follower_count", opt(followers)),
        (
            "thumbnails",
            thumbnails(renderer.get("thumbnail"), "thumbnails"),
        ),
        (
            "description",
            opt(text_of(renderer, &["descriptionSnippet"])),
        ),
    ]))
}

fn playlist(
    id: &str,
    title: Option<String>,
    channel: Option<String>,
    count: Option<u64>,
    thumbnails: Value,
) -> Value {
    entry(vec![
        ("_type", "url".into()),
        ("ie_key", "YoutubeTab".into()),
        ("id", id.into()),
        (
            "url",
            format!("https://www.youtube.com/playlist?list={id}").into(),
        ),
        ("title", opt(title)),
        ("channel", opt(channel.clone())),
        ("uploader", opt(channel)),
        ("playlist_count", opt(count)),
        ("thumbnails", thumbnails),
    ])
}

/// yt-dlp's `_extract_lockup_view_model`: videos, playlists and podcasts.
fn lockup(model: &Value) -> Option<Value> {
    let id = model.get("contentId")?.as_str()?;
    let kind = model
        .get("contentType")
        .and_then(Value::as_str)
        .unwrap_or("");
    let metadata = at(model, &["metadata", "lockupMetadataViewModel"]);
    let content = metadata.and_then(|m| at(m, &["metadata", "contentMetadataViewModel"]));
    let rows = content
        .and_then(|c| c.get("metadataRows"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let title = metadata
        .and_then(|m| at(m, &["title", "content"]))
        .and_then(Value::as_str)
        .map(str::to_owned);
    // The part of a row that links to a channel names it.
    let channel_part = rows
        .iter()
        .filter_map(|r| r.get("metadataParts")?.as_array())
        .flatten()
        .find(|p| {
            at(p, &["text", "commandRuns"])
                .and_then(Value::as_array)
                .and_then(|r| r.first())
                .and_then(|r| {
                    at(
                        r,
                        &["onTap", "innertubeCommand", "browseEndpoint", "browseId"],
                    )
                })
                .is_some()
        });
    let channel = channel_part
        .and_then(|p| at(p, &["text", "content"])?.as_str())
        .map(str::to_owned);
    let command = channel_part
        .and_then(|p| at(p, &["text", "commandRuns"])?.as_array()?.first())
        .and_then(|r| at(r, &["onTap", "innertubeCommand", "browseEndpoint"]));
    let channel_id = command
        .and_then(|c| c.get("browseId")?.as_str())
        .filter(|id| is_channel_id(id));
    let handle = command
        .and_then(|c| c.get("canonicalBaseUrl")?.as_str())
        .and_then(handle_from_url);
    match kind {
        "LOCKUP_CONTENT_TYPE_VIDEO" => {}
        "LOCKUP_CONTENT_TYPE_PLAYLIST" | "LOCKUP_CONTENT_TYPE_PODCAST" => {
            let image = at(
                model,
                &[
                    "contentImage",
                    "collectionThumbnailViewModel",
                    "primaryThumbnail",
                    "thumbnailViewModel",
                    "image",
                ],
            );
            return Some(playlist(
                id,
                title,
                channel,
                None,
                thumbnails(image, "sources"),
            ));
        }
        _ => return None,
    }
    let mut badges = Vec::new();
    for overlay in at(model, &["contentImage", "thumbnailViewModel", "overlays"])
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for list in [
            at(overlay, &["thumbnailBottomOverlayViewModel", "badges"]),
            at(
                overlay,
                &["thumbnailOverlayBadgeViewModel", "thumbnailBadges"],
            ),
        ] {
            badges.extend(
                list.and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|b| b.get("thumbnailBadgeViewModel")),
            );
        }
    }
    let duration_text = badges
        .iter()
        .find_map(|b| b.get("text")?.as_str())
        .map(str::to_lowercase);
    let styles: Vec<&str> = badges
        .iter()
        .filter_map(|b| b.get("badgeStyle")?.as_str())
        .collect();
    let views_and_time: Vec<&Value> = rows
        .iter()
        .filter_map(|r| r.get("metadataParts")?.as_array())
        .find(|parts| {
            parts
                .last()
                .is_some_and(|p| p.get("accessibilityLabel").is_some())
        })
        .map(|parts| parts.iter().collect())
        .unwrap_or_default();
    let relative = views_and_time
        .last()
        .and_then(|p| at(p, &["text", "content"])?.as_str())
        .map(str::to_lowercase);
    let views = if views_and_time.len() == 2 {
        views_and_time[0]
            .get("text")
            .and_then(|t| t.get("content")?.as_str())
            .and_then(count)
    } else {
        None
    };
    let live_status = if duration_text.as_deref() == Some("upcoming") {
        Some("is_upcoming")
    } else if styles.contains(&"THUMBNAIL_OVERLAY_BADGE_STYLE_LIVE") {
        Some("is_live")
    } else if relative.as_deref().is_some_and(|r| r.contains("streamed")) {
        Some("was_live")
    } else {
        None
    };
    // About when it was published, as for a videoRenderer.
    let published = if matches!(live_status, Some("is_live" | "is_upcoming")) {
        None
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        relative
            .as_deref()
            .and_then(ago)
            .and_then(|seconds| now.checked_sub(seconds))
    };
    Some(entry(vec![
        ("_type", "url".into()),
        ("ie_key", "Youtube".into()),
        ("id", id.into()),
        (
            "url",
            format!("https://www.youtube.com/watch?v={id}").into(),
        ),
        ("timestamp", opt(published)),
        ("title", opt(title)),
        ("duration", opt(duration_text.as_deref().and_then(seconds))),
        ("view_count", opt(views)),
        ("live_status", opt(live_status)),
        ("channel", opt(channel.clone())),
        ("uploader", opt(channel)),
        ("channel_id", opt(channel_id)),
        ("uploader_id", opt(handle)),
        (
            "availability",
            opt(members_only(model).then_some("subscriber_only")),
        ),
    ]))
}

/// yt-dlp's `_rich_entries`: a channel tab's grid item.
fn rich(item: &Value) -> Option<Value> {
    let content = item.get("content")?;
    if let Some(model @ Value::Object(_)) = content.get("lockupViewModel") {
        return lockup(model);
    }
    let renderer = [
        "videoRenderer",
        "reelItemRenderer",
        "playlistRenderer",
        "shortsLockupViewModel",
    ]
    .iter()
    .find_map(|k| content.get(*k).filter(|v| v.is_object()))?;
    if renderer.get("videoId").and_then(Value::as_str).is_some() {
        return video(renderer);
    }
    if let Some(id) = renderer.get("playlistId").and_then(Value::as_str) {
        return Some(playlist(
            id,
            text_of(renderer, &["title"]),
            None,
            None,
            Value::Array(Vec::new()),
        ));
    }
    renderer.get("entityId")?;
    let id = at(
        renderer,
        &["onTap", "innertubeCommand", "reelWatchEndpoint", "videoId"],
    )?
    .as_str()?;
    let title = at(renderer, &["overlayMetadata", "primaryText", "content"])
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            let label = renderer.get("accessibilityText")?.as_str()?;
            let cut = label
                .rfind(" views - play Short")
                .or_else(|| label.rfind(" view - play Short"))?;
            let comma = label[..cut].rfind(", ")?;
            Some(label[..comma].to_owned())
        });
    let views = at(renderer, &["overlayMetadata", "secondaryText", "content"])
        .and_then(Value::as_str)
        .and_then(count);
    Some(entry(vec![
        ("_type", "url".into()),
        ("ie_key", "Youtube".into()),
        ("id", id.into()),
        ("url", format!("https://www.youtube.com/shorts/{id}").into()),
        ("title", opt(title)),
        ("view_count", opt(views)),
    ]))
}

/// A flat item that names one video, channel or playlist.
fn basic(item: &Value) -> Option<Value> {
    let item = item.as_object()?;
    if let Some(model @ Value::Object(_)) = item.get("lockupViewModel") {
        return lockup(model);
    }
    let renderer = item
        .iter()
        .find(|(key, value)| {
            value.is_object()
                && ([
                    "playlistRenderer",
                    "videoRenderer",
                    "channelRenderer",
                    "showRenderer",
                    "reelItemRenderer",
                ]
                .contains(&key.as_str())
                    || key.starts_with("grid") && key.ends_with("Renderer"))
        })
        .map(|(_, v)| v)?;
    if let Some(id) = renderer.get("playlistId").and_then(Value::as_str) {
        let count = text_of(
            renderer,
            &["videoCount", "videoCountText", "videoCountShortText"],
        )
        .and_then(|t| count(&t))
        .or_else(|| {
            renderer
                .get("videoCount")
                .and_then(|v| v.as_str()?.parse().ok())
        });
        let channel = text_of(
            renderer,
            &["shortBylineText", "longBylineText", "ownerText"],
        );
        let thumbs = renderer
            .get("thumbnails")
            .and_then(Value::as_array)
            .and_then(|t| t.first())
            .map_or(Value::Array(Vec::new()), |t| {
                thumbnails(Some(t), "thumbnails")
            });
        return Some(playlist(
            id,
            text_of(renderer, &["title"]),
            channel,
            count,
            thumbs,
        ));
    }
    if renderer.get("videoId").is_some() {
        return video(renderer);
    }
    if renderer.get("channelId").is_some() {
        return channel(renderer);
    }
    None
}

/// The continuation token of a `continuationItemRenderer`.
fn continuation(item: &Value) -> Option<String> {
    let renderer = item.get("continuationItemRenderer")?;
    let endpoint = renderer
        .get("continuationEndpoint")
        .or_else(|| at(renderer, &["button", "buttonRenderer", "command"]))?;
    let mut commands: Vec<&Value> = at(endpoint, &["commandExecutorCommand", "commands"])
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect();
    commands.push(endpoint);
    commands
        .iter()
        .find_map(|c| at(c, &["continuationCommand", "token"])?.as_str())
        .map(str::to_owned)
}

#[derive(Serialize, Debug, Default, PartialEq)]
pub struct Page {
    pub entries: Vec<Value>,
    pub continuation: Option<String>,
    /// Browse requests (`browseId`, `params`) for the whole of each row a
    /// channel tab shows only the start of.
    pub shelves: Vec<Value>,
}

fn json_shelf(endpoint: &Value) -> Option<Value> {
    let id = endpoint.get("browseId")?.as_str()?;
    let params = endpoint.get("params")?.as_str()?;
    Some(serde_json::json!({"browseId": id, "params": params}))
}

fn contents(list: &[Value], page: &mut Page) {
    for content in list {
        if let Some(token) = continuation(content) {
            page.continuation = Some(token);
        } else if let Some(item) = content.get("richItemRenderer") {
            page.entries.extend(rich(item));
        } else if let Some(renderer) = content.get("playlistVideoRenderer") {
            page.entries.extend(video(renderer));
        } else if let Some(section) = content.get("itemSectionRenderer").or_else(|| {
            content
                .get("richSectionRenderer")
                .and_then(|s| s.get("content"))
        }) {
            for item in section
                .get("contents")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|v| v.is_object())
            {
                if let Some(grid) = item
                    .get("gridRenderer")
                    .or_else(|| item.get("reelShelfRenderer"))
                {
                    contents(
                        grid.get("items")
                            .and_then(Value::as_array)
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                        page,
                    );
                } else if let Some(shelf) = item.get("shelfRenderer") {
                    let inner = [
                        "verticalListRenderer",
                        "expandedShelfContentsRenderer",
                        "gridRenderer",
                        "horizontalListRenderer",
                    ]
                    .iter()
                    .find_map(|k| at(shelf, &["content", k, "items"]));
                    page.entries.extend(
                        inner
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(basic),
                    );
                    // A row shows only its first items; its title links to
                    // the whole list.
                    if let Some(endpoint) = at(shelf, &["endpoint", "browseEndpoint"]).filter(|e| {
                        e.get("params")
                            .and_then(Value::as_str)
                            .is_some_and(|p| !p.is_empty())
                    }) {
                        page.shelves.extend(json_shelf(endpoint));
                    }
                } else if let Some(grid) = item
                    .get("richGridRenderer")
                    .or_else(|| item.get("playlistVideoListRenderer"))
                {
                    contents(
                        grid.get("contents")
                            .and_then(Value::as_array)
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                        page,
                    );
                } else if let Some(entry) = basic(item) {
                    page.entries.push(entry);
                } else if let Some(token) = continuation(item) {
                    page.continuation = Some(token);
                }
            }
        } else if let Some(grid) = content.get("richGridRenderer") {
            contents(
                grid.get("contents")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                page,
            );
        } else if let Some(entry) = basic(content) {
            // Grid items, and the grid's continuation pages.
            page.entries.push(entry);
        }
    }
}

/// The entries and next token of a `/search` or `/browse` response, first
/// page or continuation.
pub fn page(response: &Value) -> Page {
    let mut page = Page::default();
    let mut lists: Vec<&Value> = Vec::new();
    lists.extend(at(
        response,
        &[
            "contents",
            "twoColumnSearchResultsRenderer",
            "primaryContents",
            "sectionListRenderer",
            "contents",
        ],
    ));
    for tab in at(
        response,
        &["contents", "twoColumnBrowseResultsRenderer", "tabs"],
    )
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    {
        let Some(renderer) = tab
            .get("tabRenderer")
            .filter(|t| t.get("selected") == Some(&Value::Bool(true)))
        else {
            continue;
        };
        lists.extend(at(renderer, &["content", "richGridRenderer", "contents"]));
        lists.extend(at(
            renderer,
            &["content", "sectionListRenderer", "contents"],
        ));
    }
    for key in ["onResponseReceivedCommands", "onResponseReceivedActions"] {
        for command in response
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            lists.extend(at(
                command,
                &["appendContinuationItemsAction", "continuationItems"],
            ));
            lists.extend(at(
                command,
                &["reloadContinuationItemsCommand", "continuationItems"],
            ));
        }
    }
    for list in lists.iter().filter_map(|l| l.as_array()) {
        contents(list, &mut page);
    }
    let mut seen = std::collections::HashSet::new();
    page.entries.retain(|e| {
        e.get("id")
            .and_then(Value::as_str)
            .is_none_or(|id| seen.insert(id.to_owned()))
    });
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_how_long_ago() {
        assert_eq!(ago("3 days ago"), Some(3 * 86_400));
        assert_eq!(ago("Streamed 2 weeks ago"), Some(14 * 86_400));
        assert_eq!(ago("1.7B views • 15 years ago"), Some(15 * 365 * 86_400));
        assert_eq!(ago("1 hour ago"), Some(3_600));
        assert_eq!(ago("Premiered Jan 3, 2020"), None);
        assert_eq!(ago("ago"), None);
    }

    #[test]
    fn reads_counts_as_yt_dlp_does() {
        assert_eq!(count("1,234,567 views"), Some(1_234_567));
        assert_eq!(count("1.2M views"), Some(1_200_000));
        assert_eq!(count("12K subscribers"), Some(12_000));
        assert_eq!(count("3.4B views"), Some(3_400_000_000));
        assert_eq!(count("No views"), Some(0));
        assert_eq!(count("1.5K watching"), Some(1500));
        assert_eq!(count("@handle"), None);
    }

    fn search_response() -> Value {
        json!({
            "contents": {"twoColumnSearchResultsRenderer": {"primaryContents": {"sectionListRenderer": {"contents": [
                {"itemSectionRenderer": {"contents": [
                    {"channelRenderer": {
                        "channelId": "UCxxxxxxxxxxxxxxxxxxxxxx", "title": {"simpleText": "Chan"},
                        "subscriberCountText": {"simpleText": "@chan"}, "videoCountText": {"simpleText": "1.2M subscribers"},
                        "navigationEndpoint": {"browseEndpoint": {"canonicalBaseUrl": "/@chan"}},
                        "thumbnail": {"thumbnails": [{"url": "//yt3.ggpht.com/s"}, {"url": "//yt3.ggpht.com/l"}]},
                        "descriptionSnippet": {"runs": [{"text": "About"}]}
                    }},
                    {"videoRenderer": {
                        "videoId": "abcdefghijk", "title": {"runs": [{"text": "A video"}]},
                        "ownerText": {"runs": [{"text": "Chan"}]},
                        "shortBylineText": {"runs": [{"text": "Chan", "navigationEndpoint": {"browseEndpoint": {"browseId": "UCxxxxxxxxxxxxxxxxxxxxxx", "canonicalBaseUrl": "/@chan"}}}]},
                        "lengthText": {"simpleText": "12:34"}, "viewCountText": {"simpleText": "1,234 views"},
                        "publishedTimeText": {"simpleText": "3 days ago"},
                        "detailedMetadataSnippets": [{"snippetText": {"runs": [{"text": "Snippet"}]}}]
                    }},
                    {"videoRenderer": {
                        "videoId": "livevideo01", "title": {"runs": [{"text": "Live"}]},
                        "badges": [{"metadataBadgeRenderer": {"style": "BADGE_STYLE_TYPE_LIVE_NOW"}}],
                        "viewCountText": {"runs": [{"text": "1.5K"}, {"text": " watching"}]}
                    }},
                    {"videoRenderer": {
                        "videoId": "shortvideo1", "title": {"runs": [{"text": "Short"}]},
                        "navigationEndpoint": {"commandMetadata": {"webCommandMetadata": {"url": "/shorts/shortvideo1"}}},
                        "thumbnailOverlays": [{"thumbnailOverlayTimeStatusRenderer": {"style": "SHORTS", "text": {"simpleText": "0:45"}}}]
                    }},
                    {"lockupViewModel": {
                        "contentId": "PLxxxxxxxxxxxx", "contentType": "LOCKUP_CONTENT_TYPE_PLAYLIST",
                        "contentImage": {"collectionThumbnailViewModel": {"primaryThumbnail": {"thumbnailViewModel": {"image": {"sources": [{"url": "https://i.ytimg.com/p.jpg"}]}}}}},
                        "metadata": {"lockupMetadataViewModel": {"title": {"content": "A playlist"}}}
                    }},
                    {"shelfRenderer": {"content": {"verticalListRenderer": {"items": [
                        {"videoRenderer": {"videoId": "shelfvideo1", "title": {"simpleText": "From a shelf"}}},
                        {"videoRenderer": {"videoId": "abcdefghijk", "title": {"simpleText": "Again"}}}
                    ]}}}},
                    {"adSlotRenderer": {}}
                ]}},
                {"continuationItemRenderer": {"continuationEndpoint": {"continuationCommand": {"token": "NEXT"}}}}
            ]}}}}
        })
    }

    #[test]
    fn reads_search_results() {
        let page = page(&search_response());
        assert_eq!(page.continuation.as_deref(), Some("NEXT"));
        let ids: Vec<&str> = page
            .entries
            .iter()
            .map(|e| e["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            [
                "UCxxxxxxxxxxxxxxxxxxxxxx",
                "abcdefghijk",
                "livevideo01",
                "shortvideo1",
                "PLxxxxxxxxxxxx",
                "shelfvideo1"
            ]
        );
        let channel = &page.entries[0];
        assert_eq!(channel["ie_key"], "YoutubeTab");
        assert_eq!(channel["uploader_id"], "@chan");
        assert_eq!(channel["channel_follower_count"], 1_200_000);
        assert_eq!(channel["thumbnails"][1]["url"], "//yt3.ggpht.com/l");
        let mut video = page.entries[1].clone();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let published = video
            .as_object_mut()
            .unwrap()
            .remove("timestamp")
            .and_then(|t| t.as_u64())
            .unwrap();
        assert!(now - published >= 3 * 86_400 && now - published < 3 * 86_400 + 60);
        assert_eq!(
            &video,
            &json!({
                "_type": "url", "ie_key": "Youtube", "id": "abcdefghijk", "url": "https://www.youtube.com/watch?v=abcdefghijk",
                "title": "A video", "description": "Snippet", "duration": 754, "channel_id": "UCxxxxxxxxxxxxxxxxxxxxxx",
                "channel": "Chan", "uploader": "Chan", "uploader_id": "@chan", "view_count": 1234
            })
        );
        assert_eq!(page.entries[2]["live_status"], "is_live");
        assert!(page.entries.iter().all(|e| e.get("availability").is_none()));
        assert_eq!(page.entries[2]["concurrent_view_count"], 1500);
        assert!(page.entries[2].get("view_count").is_none());
        assert_eq!(
            page.entries[3]["url"],
            "https://www.youtube.com/shorts/shortvideo1"
        );
        assert_eq!(page.entries[3]["duration"], 45);
        assert_eq!(
            page.entries[4]["url"],
            "https://www.youtube.com/playlist?list=PLxxxxxxxxxxxx"
        );
        assert_eq!(page.entries[4]["ie_key"], "YoutubeTab");
    }

    #[test]
    fn reads_channel_tabs_and_continuations() {
        let tab = json!({"contents": {"twoColumnBrowseResultsRenderer": {"tabs": [
            {"tabRenderer": {"title": "Home"}},
            {"tabRenderer": {"selected": true, "content": {"richGridRenderer": {"contents": [
                {"richItemRenderer": {"content": {"lockupViewModel": {
                    "contentId": "lockupvid01", "contentType": "LOCKUP_CONTENT_TYPE_VIDEO",
                    "contentImage": {"thumbnailViewModel": {"overlays": [{"thumbnailBottomOverlayViewModel": {"badges": [{"thumbnailBadgeViewModel": {"text": "1:02:03"}}]}}]}},
                    "metadata": {"lockupMetadataViewModel": {"title": {"content": "Lockup"}, "metadata": {"contentMetadataViewModel": {"metadataRows": [
                        {"metadataParts": [{"text": {"content": "12K views"}}, {"text": {"content": "Streamed 2 days ago"}, "accessibilityLabel": "Streamed 2 days ago"}]}
                    ]}}}}
                }}}},
                {"richItemRenderer": {"content": {"shortsLockupViewModel": {
                    "entityId": "shorts-shelf-item-x", "onTap": {"innertubeCommand": {"reelWatchEndpoint": {"videoId": "shortsvid01"}}},
                    "accessibilityText": "Funny cat, 1.2 million views - play Short",
                    "overlayMetadata": {"secondaryText": {"content": "1.2M views"}}
                }}}},
                {"continuationItemRenderer": {"continuationEndpoint": {"continuationCommand": {"token": "MORE"}}}}
            ]}}}}
        ]}}});
        let first = page(&tab);
        assert_eq!(first.continuation.as_deref(), Some("MORE"));
        assert_eq!(first.entries[0]["duration"], 3723);
        assert_eq!(first.entries[0]["view_count"], 12_000);
        assert_eq!(first.entries[0]["live_status"], "was_live");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let published = first.entries[0]["timestamp"].as_u64().unwrap();
        assert!(now - published >= 2 * 86_400 && now - published < 2 * 86_400 + 60);
        assert_eq!(
            first.entries[1]["url"],
            "https://www.youtube.com/shorts/shortsvid01"
        );
        assert_eq!(first.entries[1]["title"], "Funny cat");
        assert_eq!(first.entries[1]["view_count"], 1_200_000);

        let more = json!({"onResponseReceivedActions": [{"appendContinuationItemsAction": {"continuationItems": [
            {"richItemRenderer": {"content": {"videoRenderer": {"videoId": "nextvideo01", "title": {"simpleText": "Next"}}}}}
        ]}}]});
        let next = page(&more);
        assert_eq!(next.entries.len(), 1);
        assert_eq!(next.continuation, None);
    }

    #[test]
    fn reads_playlist_rows_and_links_to_each_whole_row() {
        let tab = json!({"contents": {"twoColumnBrowseResultsRenderer": {"tabs": [{"tabRenderer": {"selected": true, "content": {"sectionListRenderer": {"contents": [
            {"itemSectionRenderer": {"contents": [{"shelfRenderer": {
                "endpoint": {"browseEndpoint": {"browseId": "UCxxxxxxxxxxxxxxxxxxxxxx", "params": "SHELF"}},
                "content": {"horizontalListRenderer": {"items": [
                    {"lockupViewModel": {"contentId": "PLrowrowrow1", "contentType": "LOCKUP_CONTENT_TYPE_PLAYLIST", "metadata": {"lockupMetadataViewModel": {"title": {"content": "Genesis"}}}}},
                    {"gridPlaylistRenderer": {"playlistId": "PLrowrowrow2", "title": {"simpleText": "Exodus"}}}
                ]}}
            }}]}},
            {"itemSectionRenderer": {"contents": [{"shelfRenderer": {"content": {"horizontalListRenderer": {"items": [
                {"lockupViewModel": {"contentId": "PLrowrowrow3", "contentType": "LOCKUP_CONTENT_TYPE_PLAYLIST"}}
            ]}}}}]}}
        ]}}}}]}}});
        let read = page(&tab);
        let ids: Vec<&str> = read
            .entries
            .iter()
            .filter_map(|e| e["id"].as_str())
            .collect();
        assert_eq!(ids, ["PLrowrowrow1", "PLrowrowrow2", "PLrowrowrow3"]);
        assert_eq!(read.entries[0]["title"], "Genesis");
        assert_eq!(
            read.shelves,
            [json!({"browseId": "UCxxxxxxxxxxxxxxxxxxxxxx", "params": "SHELF"})]
        );

        let grid = json!({"contents": {"twoColumnBrowseResultsRenderer": {"tabs": [{"tabRenderer": {"selected": true, "content": {"sectionListRenderer": {"contents": [
            {"itemSectionRenderer": {"contents": [{"gridRenderer": {"items": [
                {"lockupViewModel": {"contentId": "PLgridgrid01", "contentType": "LOCKUP_CONTENT_TYPE_PLAYLIST"}},
                {"continuationItemRenderer": {"continuationEndpoint": {"continuationCommand": {"token": "GRID"}}}}
            ]}}]}}
        ]}}}}]}}});
        let first = page(&grid);
        assert_eq!(first.entries[0]["id"], "PLgridgrid01");
        assert_eq!(first.continuation.as_deref(), Some("GRID"));
        let more = page(
            &json!({"onResponseReceivedActions": [{"appendContinuationItemsAction": {"continuationItems": [
                {"lockupViewModel": {"contentId": "PLgridgrid02", "contentType": "LOCKUP_CONTENT_TYPE_PLAYLIST"}}
            ]}}]}),
        );
        assert_eq!(more.entries[0]["id"], "PLgridgrid02");
    }

    #[test]
    fn reads_nothing_from_other_responses() {
        assert_eq!(page(&json!({"error": {"code": 400}})), Page::default());
    }

    #[test]
    fn reads_playlist_pages() {
        let first = json!({"contents": {"twoColumnBrowseResultsRenderer": {"tabs": [{"tabRenderer": {"selected": true, "content": {"sectionListRenderer": {"contents": [
            {"itemSectionRenderer": {"contents": [{"playlistVideoListRenderer": {"contents": [
                {"playlistVideoRenderer": {
                    "videoId": "dQw4w9WgXcQ",
                    "title": {"runs": [{"text": "Never Gonna Give You Up"}]},
                    "shortBylineText": {"runs": [{"text": "Rick Astley", "navigationEndpoint": {"browseEndpoint": {"browseId": "UCuAXFkgsw1L7xaCfnd5JJOw", "canonicalBaseUrl": "/@RickAstleyYT"}}}]},
                    "lengthSeconds": "213",
                    "videoInfo": {"runs": [{"text": "1.7B views"}, {"text": " • "}, {"text": "15 years ago"}]}
                }},
                {"continuationItemRenderer": {"continuationEndpoint": {"continuationCommand": {"token": "NEXT"}}}}
            ]}}]}}
        ]}}}}]}}});
        let page = page(&first);
        assert_eq!(page.continuation.as_deref(), Some("NEXT"));
        assert_eq!(page.entries.len(), 1);
        let video = &page.entries[0];
        assert_eq!(video["id"], "dQw4w9WgXcQ");
        assert_eq!(video["title"], "Never Gonna Give You Up");
        assert_eq!(video["duration"], 213);
        assert_eq!(video["channel"], "Rick Astley");
        assert_eq!(video["channel_id"], "UCuAXFkgsw1L7xaCfnd5JJOw");
        assert_eq!(video["view_count"], 1_700_000_000u64);
        let more = json!({"onResponseReceivedActions": [{"appendContinuationItemsAction": {"continuationItems": [
            {"playlistVideoRenderer": {"videoId": "aaaaaaaaaaa", "title": {"runs": [{"text": "Next"}]}}}
        ]}}]});
        assert_eq!(super::page(&more).entries[0]["id"], "aaaaaaaaaaa");
    }

    #[test]
    fn marks_members_only_entries() {
        let renderer = json!({"videoId": "aaaaaaaaaaa", "title": {"runs": [{"text": "Members"}]},
            "badges": [{"metadataBadgeRenderer": {"style": "BADGE_STYLE_TYPE_MEMBERS_ONLY", "label": "Members only"}}]});
        assert_eq!(video(&renderer).unwrap()["availability"], "subscriber_only");
        let model = json!({"contentId": "bbbbbbbbbbb", "contentType": "LOCKUP_CONTENT_TYPE_VIDEO",
        "metadata": {"lockupMetadataViewModel": {"title": {"content": "Members"}, "metadata": {"contentMetadataViewModel": {"metadataRows": [
            {"badges": [{"badgeViewModel": {"badgeText": "Members only", "badgeStyle": "BADGE_MEMBERS_ONLY"}}]}
        ]}}}}});
        assert_eq!(lockup(&model).unwrap()["availability"], "subscriber_only");
    }
}
