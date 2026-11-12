package app.yato.player

import android.app.Activity
import android.app.ActivityOptions
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.app.NotificationCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File
import kotlin.concurrent.thread

@InvokeArg
class InstallArgs {
  /** L'APK déjà téléchargé par le cœur Rust (src-tauri/src/android_update.rs). */
  var path: String = ""
}

/**
 * Mises à jour intégrées : installe l'APK de la nouvelle version, téléchargé par le cœur.
 *
 * L'installation passe par une session [PackageInstaller] :
 * - la toute première fois, Android demande d'autoriser 555 à installer des applis (réglage
 *   ouvert ici, `install` échoue avec `permission` : l'interface réessaie au retour) ;
 * - tant que 555 a été installé par le navigateur, Android affiche « Mettre à jour ? » ;
 * - dès que 555 s'est mis à jour lui-même une fois (Android 12 et plus), les suivantes se font
 *   sans confirmation.
 * Android vérifie lui-même que l'APK est signé avec la même clé que l'appli installée.
 *
 * Android ferme 555 pour le remplacer. Pour le rouvrir, le résultat de la session est adressé à
 * [MainActivity] : c'est Android qui l'envoie, et certains téléphones l'autorisent alors à
 * ouvrir l'activité. D'autres le refusent (testé : Vivo, Android 13) ; [UpdatedReceiver]
 * propose alors de rouvrir 555 par une notification.
 */
@TauriPlugin
class UpdatePlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun install(invoke: Invoke) {
    val args = invoke.parseArgs(InstallArgs::class.java)
    val ctx = activity.applicationContext
    if (!ctx.packageManager.canRequestPackageInstalls()) {
      activity.startActivity(
        Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${ctx.packageName}")),
      )
      invoke.reject("permission")
      return
    }
    val apk = File(args.path)
    thread(name = "yato-update-install") {
      try {
        val installer = ctx.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
        params.setAppPackageName(ctx.packageName)
        if (Build.VERSION.SDK_INT >= 31) {
          params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
        }
        val id = installer.createSession(params)
        installer.openSession(id).use { session ->
          apk.inputStream().use { input ->
            session.openWrite("555.apk", 0, apk.length()).use { out ->
              input.copyTo(out)
              session.fsync(out)
            }
          }
          pending?.reject("superseded")
          pending = invoke
          File(ctx.noBackupFilesDir, UPDATING).writeText(apk.name)
          session.commit(statusIntent(ctx, id).intentSender)
        }
      } catch (e: Exception) {
        pending = null
        invoke.reject(e.message ?: e.toString())
      }
    }
  }

  companion object {
    /** Action de l'intent qui porte le résultat de la session jusqu'à [MainActivity]. */
    const val ACTION_STATUS = "app.yato.player.UPDATE_STATUS"

    /** L'appel `install` en cours : réussi, l'appli est remplacée ; sinon, l'erreur lui revient. */
    @Volatile
    var pending: Invoke? = null

    /** Marque une mise à jour lancée par 555 (pas une installation par adb ou le navigateur). */
    const val UPDATING = "updating"

    private fun statusIntent(ctx: Context, id: Int): PendingIntent {
      val intent = Intent(ctx, MainActivity::class.java).setAction(ACTION_STATUS)
      val flags = PendingIntent.FLAG_UPDATE_CURRENT or
        (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
      // Android 14 et plus : ouvrir l'activité depuis l'arrière-plan doit être autorisé ici.
      val options = if (Build.VERSION.SDK_INT >= 34) {
        ActivityOptions.makeBasic()
          .setPendingIntentCreatorBackgroundActivityStartMode(ActivityOptions.MODE_BACKGROUND_ACTIVITY_START_ALLOWED)
          .toBundle()
      } else {
        null
      }
      return PendingIntent.getActivity(ctx, id, intent, flags, options)
    }

    /**
     * Résultat de la session, reçu par [MainActivity]. Renvoie `true` si l'intent en était un.
     * Mise à jour réussie : rien à faire, l'activité qui s'ouvre est déjà la nouvelle version.
     */
    fun onStatus(activity: Activity, intent: Intent?): Boolean {
      if (intent?.action != ACTION_STATUS) return false
      when (val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
        PackageInstaller.STATUS_PENDING_USER_ACTION -> {
          // « Mettre à jour ? » : la fenêtre d'Android, au-dessus de 555.
          @Suppress("DEPRECATION")
          val confirm = intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)
          if (confirm != null) activity.startActivity(confirm)
        }
        PackageInstaller.STATUS_SUCCESS -> pending = null
        else -> {
          val message = if (status == PackageInstaller.STATUS_FAILURE_ABORTED) {
            "cancelled"
          } else {
            intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE) ?: "status $status"
          }
          File(activity.noBackupFilesDir, UPDATING).delete()
          pending?.reject(message)
          pending = null
        }
      }
      return true
    }
  }
}

/**
 * 555 vient d'être mis à jour par lui-même : si Android n'a pas permis de le rouvrir, une
 * notification le propose d'un toucher ([MainActivity] la retire dès qu'il est ouvert).
 */
class UpdatedReceiver : BroadcastReceiver() {
  override fun onReceive(ctx: Context, intent: Intent) {
    if (intent.action != Intent.ACTION_MY_PACKAGE_REPLACED) return
    if (!File(ctx.noBackupFilesDir, UpdatePlugin.UPDATING).delete()) return
    if (MainActivity.visible) return
    val nm = ctx.getSystemService(NotificationManager::class.java)
    if (!nm.areNotificationsEnabled()) return
    val open = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName) ?: return
    nm.createNotificationChannel(
      NotificationChannel(CHANNEL_ID, ctx.getString(R.string.updates_channel), NotificationManager.IMPORTANCE_DEFAULT),
    )
    val version = ctx.packageManager.getPackageInfo(ctx.packageName, 0).versionName ?: ""
    val notification = NotificationCompat.Builder(ctx, CHANNEL_ID)
      .setSmallIcon(R.drawable.ic_stat_555)
      .setContentTitle(ctx.getString(R.string.updated_title, version))
      .setContentText(ctx.getString(R.string.updated_text))
      .setAutoCancel(true)
      .setContentIntent(PendingIntent.getActivity(ctx, 0, open, PendingIntent.FLAG_IMMUTABLE))
      .build()
    nm.notify(NOTIFICATION_ID, notification)
  }

  companion object {
    private const val CHANNEL_ID = "updates"
    const val NOTIFICATION_ID = 555
  }
}
