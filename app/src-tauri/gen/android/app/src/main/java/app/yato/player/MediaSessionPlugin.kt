package app.yato.player

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class InitArgs {
  lateinit var channel: Channel
}

/** Morceau en cours et état de lecture, envoyés par le cœur Rust (src-tauri/src/android_media.rs). */
@InvokeArg
class UpdateArgs {
  var active: Boolean = false
  var title: String = ""
  var artist: String = ""
  var album: String? = null
  /** Pochette : adresse https:// ou file://. */
  var cover: String? = null
  /** En secondes. */
  var duration: Double = 0.0
  var position: Double = 0.0
  var playing: Boolean = false
}

/**
 * Pont entre le cœur Rust et la lecture Android en arrière-plan ([PlaybackService]) :
 * - `init` reçoit le canal par lequel les commandes (notification, écran verrouillé, casque…)
 *   repartent vers le cœur ;
 * - `update` reçoit chaque changement de morceau ou d'état.
 * Le tout fonctionne écran éteint : rien ne passe par l'interface (la WebView est alors en pause).
 */
@TauriPlugin
class MediaSessionPlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun init(invoke: Invoke) {
    val args = invoke.parseArgs(InitArgs::class.java)
    PlaybackService.onCommand = { command -> args.channel.send(command) }
    invoke.resolve()
  }

  @Command
  fun update(invoke: Invoke) {
    val args = invoke.parseArgs(UpdateArgs::class.java)
    PlaybackService.update(activity.applicationContext, args)
    invoke.resolve()
  }
}
