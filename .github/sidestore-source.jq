# SideStore (and AltStore) source for 555, filled in by the release workflow (job `ios`).
# Format: https://faq.altstore.io/developers/make-a-source
{
  name: "555",
  identifier: "app.yato.player.source",
  subtitle: "A light, fast music player.",
  description: "The iPhone and iPad version of 555. Add this source in SideStore to get new versions on your phone.",
  iconURL: "https://555.amon.lol/assets/apple-touch-icon.png",
  website: "https://555.amon.lol/",
  tintColor: "#2433FF",
  apps: [
    {
      name: "555",
      bundleIdentifier: "app.yato.player",
      developerName: "amonre555",
      subtitle: "A light, fast music player.",
      localizedDescription: "Free and open source music player: search any song, play your own files, import playlists from Spotify, Deezer or YouTube Music, with lyrics and an equalizer.",
      iconURL: "https://555.amon.lol/assets/apple-touch-icon.png",
      tintColor: "#2433FF",
      category: "entertainment",
      versions: [
        {
          version: $version,
          buildVersion: $version,
          date: $date,
          localizedDescription: $notes,
          downloadURL: $url,
          size: $size,
          minOSVersion: "15.0"
        }
      ],
      appPermissions: {
        entitlements: [],
        privacy: {}
      }
    }
  ],
  news: []
}
