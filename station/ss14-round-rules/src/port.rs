// Copyright (c) 2017-2026 Space Wizards Federation. MIT: ../../ss14-port/SS14-LICENSE.txt.
pub fn can_ready(known: bool, loaded: bool, lobby: bool) -> Result<bool, String> {
    Ok(known && loaded && lobby)
}
pub fn can_spawn(lobby_enabled: bool, ready: bool, connected: bool) -> Result<bool, String> {
    Ok((!lobby_enabled || ready) && connected)
}
