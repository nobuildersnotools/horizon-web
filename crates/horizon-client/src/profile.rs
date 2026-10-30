//! The player's profile in the browser build, as Eaglercraft's Edit Profile
//! screen keeps it: the offline name, a skin (one of the game's own default
//! skins, or a picture the player imported) and an imported cape.
//!
//! The name is the offline name's own line in `horizon-player.txt`
//! ([`crate::offline_name`]), so every join uses it; the skin and cape are
//! more lines there, with the pictures in `horizon-skin.png` and
//! `horizon-cape.png`. [`apply`] shows them on the player in the game and
//! sends them with an Eaglercraft server's login.
use horizon_os::fs::{self, PathExt};
use std::path::Path;

/// The default skins to pick from (DefaultPlayerSkin's characters).
pub const CHARACTERS: [&str; 9] = [
    "steve", "alex", "ari", "efe", "kai", "makena", "noor", "sunny", "zuri",
];
const SKIN_FILE: &str = "horizon-skin.png";
const CAPE_FILE: &str = "horizon-cape.png";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Skin {
    /// One of [`CHARACTERS`].
    Default(usize),
    /// `horizon-skin.png`.
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub skin: Skin,
    /// Three-pixel arms.
    pub slim: bool,
    /// Wears `horizon-cape.png`.
    pub cape: bool,
}

impl Profile {
    /// A first profile: the generated offline name, Steve.
    pub fn new(dir: &Path) -> Self {
        Self {
            name: crate::offline_name::load_or_create(dir),
            skin: Skin::Default(0),
            slim: false,
            cape: false,
        }
    }

    /// The skin's texture in the game's atlas, for a default one.
    pub fn default_texture(&self) -> Option<String> {
        let Skin::Default(i) = self.skin else {
            return None;
        };
        let model = if self.slim { "slim" } else { "wide" };
        Some(format!(
            "entity/player/{model}/{}",
            CHARACTERS[i % CHARACTERS.len()]
        ))
    }
}

/// The profile saved in `dir`, or `None` before the player made one.
pub fn load(dir: &Path) -> Option<Profile> {
    let text = fs::read_to_string(dir.join(crate::offline_name::FILE)).ok()?;
    let value = |key: &str| {
        text.lines()
            .find_map(|l| l.trim_end_matches('\r').strip_prefix(key))
            .map(str::to_owned)
    };
    let skin = value("skin:")?;
    let name = value("name:").filter(|n| crate::offline_name::is_valid(n))?;
    let skin = match skin.as_str() {
        "custom" if dir.join(SKIN_FILE).fs_exists() => Skin::Custom,
        other => Skin::Default(CHARACTERS.iter().position(|c| *c == other).unwrap_or(0)),
    };
    Some(Profile {
        name,
        skin,
        slim: value("slim:").as_deref() == Some("true"),
        cape: value("cape:").as_deref() == Some("true") && dir.join(CAPE_FILE).fs_exists(),
    })
}

/// Saves the profile in `dir`.
pub fn save(dir: &Path, profile: &Profile) -> std::io::Result<()> {
    let skin = match profile.skin {
        Skin::Default(i) => CHARACTERS[i % CHARACTERS.len()],
        Skin::Custom => "custom",
    };
    fs::write(
        dir.join(crate::offline_name::FILE),
        format!(
            "name:{}\nskin:{skin}\nslim:{}\ncape:{}\n",
            profile.name, profile.slim, profile.cape
        ),
    )
}

/// A skin picture the player imported, checked and kept as the custom skin:
/// 64×64, or the older 64×32.
pub fn import_skin(dir: &Path, png: &[u8]) -> Result<(), String> {
    let (w, h, rgba) = crate::hud::decode_bytes(png).map_err(|e| e.to_string())?;
    crate::skins::process_legacy_skin(w, h, &rgba).ok_or(format!(
        "A skin is 64×64 (or 64×32) pixels; this one is {w}×{h}."
    ))?;
    fs::write(dir.join(SKIN_FILE), png).map_err(|e| e.to_string())
}

/// A cape picture the player imported: 64×32 (or the older 22×17).
pub fn import_cape(dir: &Path, png: &[u8]) -> Result<(), String> {
    let (w, h, rgba) = crate::hud::decode_bytes(png).map_err(|e| e.to_string())?;
    crate::skins::process_cape(w, h, &rgba).ok_or("That picture is not a cape.")?;
    fs::write(dir.join(CAPE_FILE), png).map_err(|e| e.to_string())
}

/// The skin's 64×64 RGBA pixels: the imported picture, or the default
/// skin's texture from the resources.
pub fn skin_pixels(dir: &Path, resources: &Path, profile: &Profile) -> Option<Vec<u8>> {
    let path = match profile.default_texture() {
        Some(texture) => resources.join(format!("assets/minecraft/textures/{texture}.png")),
        None => dir.join(SKIN_FILE),
    };
    let (w, h, rgba) = crate::hud::decode_bytes(&fs::read(path).ok()?).ok()?;
    crate::skins::process_legacy_skin(w, h, &rgba)
}

/// The cape's pixels as the game's skin slots lay it out (64×64, the cape
/// in the top half).
pub fn cape_pixels(dir: &Path) -> Option<Vec<u8>> {
    let (w, h, rgba) = crate::hud::decode_bytes(&fs::read(dir.join(CAPE_FILE)).ok()?).ok()?;
    crate::skins::process_cape(w, h, &rgba)
}

/// Shows the profile's skin and cape on the player and sends them with
/// Eaglercraft logins from now on.
pub fn apply(dir: &Path, resources: &Path, profile: &Profile) {
    let skin = skin_pixels(dir, resources, profile);
    let cape = profile.cape.then(|| cape_pixels(dir)).flatten();
    horizon_protocol::eagler::set_look(
        skin.as_deref().map(|s| (s, profile.slim)),
        cape.as_deref().map(|c| &c[..64 * 32 * 4]),
    );
    let own = match (profile.default_texture(), skin) {
        (Some(texture), _) => crate::skins::OwnSkin::Default(texture),
        (None, Some(rgba)) => crate::skins::OwnSkin::Custom(rgba),
        (None, None) => crate::skins::OwnSkin::Default("entity/player/wide/steve".into()),
    };
    crate::skins::set_own_look(own, profile.slim, cape);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_round_trip_and_a_bare_name_is_no_profile() {
        let dir = horizon_os::env::temp_dir()
            .join(format!("horizon-profile-{}", horizon_os::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // The offline name alone (what native keeps) is not a profile yet.
        let first = Profile::new(&dir);
        assert_eq!(load(&dir), None);
        let profile = Profile {
            skin: Skin::Default(2),
            slim: true,
            ..first
        };
        save(&dir, &profile).unwrap();
        assert_eq!(load(&dir), Some(profile.clone()));
        assert_eq!(
            profile.default_texture().as_deref(),
            Some("entity/player/slim/ari")
        );
        assert_eq!(
            crate::offline_name::load_or_create(&dir),
            profile.name,
            "the offline name reads the profile's"
        );
        // A custom skin whose picture is gone falls back to Steve.
        save(
            &dir,
            &Profile {
                skin: Skin::Custom,
                ..profile
            },
        )
        .unwrap();
        assert_eq!(load(&dir).unwrap().skin, Skin::Default(0));
        let _ = fs::remove_dir_all(&dir);
    }
}
