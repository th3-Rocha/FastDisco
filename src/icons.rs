fastframe_icons::icons! {
    /// Every icon the interface draws.
    pub enum Icon {
        prefix: "discofast-icon-",
        directory: "../assets/icons/",
        Hash => "rc-hashtag",
        Speaker => "rc-speaker",
        Headphones => "rc-headphones",
        Mic => "rc-mic",
        MicOff => "rc-mic-off",
        HeadphonesOff => "rc-headphones-off",
        Settings => "rc-gear",
        Plus => "rc-plus",
        PlusCircle => "rc-plus-circle",
        Compass => "rc-globe",
        Download => "rc-download",
        Users => "rc-friends",
        User => "rc-user",
        UserPlus => "rc-add-friend",
        Inbox => "rc-inbox",
        Help => "rc-help",
        Bell => "rc-bell",
        Pin => "rc-pin",
        Search => "rc-search",
        Threads => "rc-threads",
        Gift => "rc-gift",
        Gif => "rc-gif",
        Smile => "rc-smile",
        ChevronDown => "rc-chevron-down",
        ChevronRight => "rc-chevron-right",
        Close => "rc-close",
        Reply => "rc-reply",
        ReplyArrow => "rc-reply-arrow",
        Pencil => "rc-pencil",
        Trash => "rc-trash",
        More => "rc-more",
        ReactAdd => "rc-react-add",
        Play => "rc-play",
        Pause => "rc-pause",
        Volume => "rc-speaker",
        VolumeOff => "rc-volume-off",
        File => "rc-file",
        FileAudio => "rc-file-audio",
        Paperclip => "rc-paperclip",
        Image => "image",
        External => "rc-external",
        Copy => "rc-copy",
        Eye => "rc-eye",
        EyeOff => "rc-eye-off",
        Music => "music-2",
        Star => "rc-star",
        StarFill => "rc-star-fill",
        Trending => "rc-trending",
        Back => "rc-back",
        Forward => "rc-forward",
        Discord => "discord",
        Check => "rc-check",
        MessageCircle => "rc-chat",
        Folder => "rc-folder",
        Calendar => "rc-calendar",
        HangUp => "rc-hang-up",
        Connection => "rc-connection",
        NoiseSuppression => "rc-noise-suppression",
        NoiseSuppressionOff => "rc-noise-suppression-off",
        Video => "rc-video",
        ScreenShare => "rc-screen-share",
        Activities => "rc-activity",
        Soundboard => "rc-soundboard",
        Send => "rc-send",
        Palette => "rc-palette",
        Moon => "rc-moon",
        Sun => "rc-sun",
        Dice => "rc-dice",
        Pipette => "rc-pipette",
        Apps => "rc-apps",
        Keyboard => "rc-keyboard",
        Accessibility => "rc-accessibility",
        Monitor => "rc-monitor",
        Shield => "rc-shield-check",
        LogOut => "rc-logout",
        Forum => "rc-forum",
        Stage => "rc-stage",
        Hand => "rc-hand",
        Tag => "rc-tag",
        Phone => "rc-phone",
        Code => "rc-code",
        Lock => "rc-lock",
        StageLock => "rc-stage-lock",
        ForumLock => "rc-forum-lock",
        SpeakerLock => "rc-speaker-lock",
        HashLock => "rc-hashtag-lock",
        ShieldLock => "rc-shield-lock",
        AddGroup => "rc-add-group",
    }
}

#[cfg(test)]
mod tests {
    /// Every icon parses and draws something, as egui's SVG loader will.
    #[test]
    fn every_icon_renders() {
        use resvg::{tiny_skia, usvg};
        for icon in super::Icon::ALL {
            let tree = usvg::Tree::from_data(icon.bytes(), &usvg::Options::default())
                .unwrap_or_else(|e| panic!("{icon:?}: {e}"));
            let mut pixmap = tiny_skia::Pixmap::new(48, 48).unwrap();
            let scale = 48.0 / tree.size().width();
            resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
            assert!(pixmap.pixels().iter().any(|p| p.alpha() > 0), "{icon:?} is empty");
        }
    }
}
