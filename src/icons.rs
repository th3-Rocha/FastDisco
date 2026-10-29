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
        PlusCircle => "circle-plus",
        Compass => "rc-globe",
        Download => "download",
        Users => "rc-friends",
        User => "rc-user",
        UserPlus => "rc-add-friend",
        Inbox => "inbox",
        Help => "circle-help",
        Bell => "rc-bell",
        Pin => lucide "pin",
        Search => "rc-search",
        Threads => "messages-square",
        Gift => "rc-gift",
        Gif => "gif",
        Smile => "smile",
        ChevronDown => "rc-chevron-down",
        ChevronRight => "rc-chevron-right",
        Close => "rc-close",
        Reply => "reply",
        ReplyArrow => "corner-down-right",
        Pencil => "rc-pencil",
        Trash => lucide "trash-2",
        More => "rc-more",
        ReactAdd => "smile-plus",
        Play => lucide "play",
        Pause => lucide "pause",
        Volume => "rc-speaker",
        VolumeOff => lucide "volume-x",
        File => "file-text",
        FileAudio => "file-audio",
        Paperclip => "paperclip",
        Image => "image",
        External => lucide "external-link",
        Copy => lucide "copy",
        Eye => lucide "eye",
        EyeOff => lucide "eye-off",
        Music => "music-2",
        Star => "star",
        StarFill => "star-fill",
        Trending => "trending-up",
        Back => lucide "arrow-left",
        Forward => "arrow-right",
        Discord => "discord",
        Check => lucide "check",
        MessageCircle => "rc-chat",
        Folder => "folder",
        Calendar => "calendar",
        HangUp => "phone-hangup",
        Connection => "rss",
        NoiseSuppression => "audio-lines",
        NoiseSuppressionOff => "audio-lines-off",
        Video => "video",
        ScreenShare => "screen-share",
        Activities => "rc-activity",
        Soundboard => "megaphone",
        Send => "send-horizontal",
        Palette => "rc-palette",
        Moon => lucide "moon",
        Sun => lucide "sun",
        Dice => "dice-5",
        Pipette => "pipette",
        Apps => "layout-grid",
        Keyboard => "rc-keyboard",
        Accessibility => "rc-accessibility",
        Monitor => "monitor",
        Shield => "rc-shield-check",
        LogOut => "rc-logout",
        Forum => "message-square-text",
        Stage => "radio",
        Hand => "hand",
        Tag => "tag",
        Phone => "phone",
        Code => "rc-code",
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
