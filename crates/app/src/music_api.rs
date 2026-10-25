#![allow(unsafe_code)]

use backspn_formats::music_library::{self, MusicLibrary, MusicPlaylist, MusicTrack};
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{exception, msg_send};
use objc2_foundation::{NSArray, NSDate, NSError, NSNumber, NSString, NSURL};

#[link(name = "iTunesLibrary", kind = "framework")]
unsafe extern "C" {}

const SONG: usize = 2;
const FILE: usize = 1;
const SMART: usize = 1;
const FOLDER: usize = 3;

pub fn read() -> Result<MusicLibrary, String> {
    exception::catch(|| autoreleasepool(|_| read_library())).map_err(|e| {
        e.map_or_else(
            || "Unknown exception during import".to_owned(),
            |e| e.to_string(),
        )
    })?
}

fn read_library() -> Result<MusicLibrary, String> {
    let class = AnyClass::get(c"ITLibrary").ok_or("Could not open the music library!")?;
    let version = NSString::from_str("1.0");
    let library: Result<Retained<AnyObject>, Retained<NSError>> =
        unsafe { msg_send![class, libraryWithAPIVersion: &*version, error: _] };
    let library = library.map_err(|e| {
        format!(
            "Could not open the music library! {}",
            e.localizedDescription()
        )
    })?;
    unsafe {
        let _: bool = msg_send![&library, reloadData];
        let items: Retained<NSArray<AnyObject>> = msg_send![&library, allMediaItems];
        let lists: Retained<NSArray<AnyObject>> = msg_send![&library, allPlaylists];
        Ok(MusicLibrary {
            tracks: items.iter().filter_map(|i| track(&i)).collect(),
            playlists: lists.iter().filter_map(|p| playlist(&p)).collect(),
        })
    }
}

unsafe fn string(object: &AnyObject, getter: objc2::runtime::Sel) -> String {
    let value: Option<Retained<NSString>> = unsafe { msg_send![object, performSelector: getter] };
    value.map(|s| s.to_string()).unwrap_or_default()
}

unsafe fn int_id(object: &AnyObject) -> i64 {
    let id: Retained<NSNumber> = unsafe { msg_send![object, persistentID] };
    i64::from(id.intValue())
}

fn day(date: Option<Retained<NSDate>>) -> String {
    date.map(|d| {
        let secs = d.timeIntervalSince1970().max(0.0) as u64;
        backspn_library::clock::LocalTime::from_unix(secs).date_text()
    })
    .unwrap_or_default()
}

unsafe fn track(item: &AnyObject) -> Option<MusicTrack> {
    unsafe {
        let media_kind: usize = msg_send![item, mediaKind];
        let video: bool = msg_send![item, isVideo];
        let location_type: usize = msg_send![item, locationType];
        let kind = string(item, objc2::sel!(kind));
        if media_kind != SONG
            || video
            || location_type != FILE
            || !music_library::is_music_kind(&kind)
        {
            return None;
        }
        let artist: Option<Retained<AnyObject>> = msg_send![item, artist];
        let album: Option<Retained<AnyObject>> = msg_send![item, album];
        let location: Option<Retained<NSURL>> = msg_send![item, location];
        let file_size: u64 = msg_send![item, fileSize];
        let bitrate: usize = msg_send![item, bitrate];
        let total_time: usize = msg_send![item, totalTime];
        let track_number: usize = msg_send![item, trackNumber];
        let bpm: usize = msg_send![item, beatsPerMinute];
        let rating: isize = msg_send![item, rating];
        Some(MusicTrack {
            id: int_id(item),
            title: string(item, objc2::sel!(title)),
            artist: artist
                .map(|a| string(&a, objc2::sel!(name)))
                .unwrap_or_default(),
            album: album
                .map(|a| string(&a, objc2::sel!(title)))
                .unwrap_or_default(),
            genre: string(item, objc2::sel!(genre)),
            comments: string(item, objc2::sel!(comments)),
            size_kib: file_size / 1024,
            bitrate: u32::try_from(bitrate).unwrap_or(0).saturating_mul(1000),
            seconds: music_library::seconds(total_time as u64),
            track_number: u32::try_from(track_number).unwrap_or(0),
            bpm: u32::try_from(bpm).unwrap_or(0),
            rating: music_library::rating(rating as i64),
            date_added: day(msg_send![item, addedDate]),
            last_played: day(msg_send![item, lastPlayedDate]),
            path: location
                .and_then(|u| u.path())
                .map(|p| p.to_string())
                .unwrap_or_default(),
        })
    }
}

unsafe fn playlist(list: &AnyObject) -> Option<MusicPlaylist> {
    unsafe {
        let distinguished: usize = msg_send![list, distinguishedKind];
        let id: Option<Retained<NSNumber>> = msg_send![list, persistentID];
        let id = id?;
        if distinguished != 0 {
            return None;
        }
        let parent: Option<Retained<NSNumber>> = msg_send![list, parentID];
        let kind: usize = msg_send![list, kind];
        let master: bool = msg_send![list, isMaster];
        let items: Retained<NSArray<AnyObject>> = msg_send![list, items];
        Some(MusicPlaylist {
            name: string(list, objc2::sel!(name)),
            id: id.unsignedLongLongValue().to_string(),
            parent: parent.map(|p| p.unsignedLongLongValue().to_string()),
            tracks: items.iter().map(|i| int_id(&i)).collect(),
            folder: kind == FOLDER,
            smart: kind == SMART,
            master,
        })
    }
}
