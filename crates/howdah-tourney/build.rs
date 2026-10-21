fn main() {
    // Windows version info for the program: an unsigned exe without any
    // looks more like malware to Defender, which has taken the daemon for
    // a trojan (Trojan:Win32/Bearfoos.A!ml).
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = tauri_winres::WindowsResource::new();
        res.set("ProductName", "Howdah")
            .set("CompanyName", "Brian Haskin Jr.")
            .set("LegalCopyright", "Copyright (c) 2026 Brian Haskin Jr. MIT License.")
            .set("InternalName", "howdah-tourney")
            .set("OriginalFilename", "howdah-tourney.exe");
        res.compile_for(&["howdah-tourney"]).expect("couldn't compile the Windows resource");
    }
}
