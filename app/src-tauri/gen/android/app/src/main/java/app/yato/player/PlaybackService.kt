package app.yato.player

import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ServiceInfo
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import androidx.media.session.MediaButtonReceiver
import app.tauri.plugin.JSObject
import java.net.URL
import kotlin.concurrent.thread

/**
 * Lecture en arrière-plan. Le son lui-même est produit par le cœur Rust (AAudio) ; ce service
 * garde l'appli en vie écran éteint (service « au premier plan » de type lecture multimédia) et
 * la relie au système :
 * - notification du lecteur, commandes de l'écran verrouillé, boutons du casque / Bluetooth
 *   (session multimédia) ;
 * - pause quand une autre appli prend le son (appel, vidéo…), reprise ensuite si c'était
 *   passager ; pause quand le casque est débranché ;
 * - processeur et Wi-Fi gardés éveillés pendant la lecture (téléchargement du morceau suivant).
 *
 * Il tourne tant qu'un morceau est chargé (en lecture ou en pause), et s'arrête avec la lecture.
 */
class PlaybackService : Service() {
  companion object {
    private const val CHANNEL_ID = "playback"
    private const val NOTIFICATION_ID = 555
    private const val ACTIONS = PlaybackStateCompat.ACTION_PLAY or PlaybackStateCompat.ACTION_PAUSE or
      PlaybackStateCompat.ACTION_PLAY_PAUSE or PlaybackStateCompat.ACTION_SKIP_TO_NEXT or
      PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS or PlaybackStateCompat.ACTION_SEEK_TO or
      PlaybackStateCompat.ACTION_STOP

    /** Commandes vers le cœur Rust (posé par [MediaSessionPlugin.init]). */
    @Volatile var onCommand: ((JSObject) -> Unit)? = null

    private val main = Handler(Looper.getMainLooper())
    private var session: MediaSessionCompat? = null
    private var state: UpdateArgs? = null
    private var art: Bitmap? = null
    private var artFor: String? = null
    private var instance: PlaybackService? = null
    private var starting = false

    private fun send(action: String, position: Double? = null) {
      val o = JSObject()
      o.put("action", action)
      if (position != null) o.put("position", position)
      onCommand?.invoke(o)
    }

    private fun openApp(context: Context): PendingIntent = PendingIntent.getActivity(
      context, 0,
      Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private fun session(context: Context): MediaSessionCompat = session ?: MediaSessionCompat(context, "555").also { s ->
      s.setCallback(object : MediaSessionCompat.Callback() {
        override fun onPlay() = send("play")
        override fun onPause() = send("pause")
        override fun onSkipToNext() = send("next")
        override fun onSkipToPrevious() = send("previous")
        override fun onStop() = send("stop")
        override fun onSeekTo(pos: Long) = send("seek", pos / 1000.0)
      })
      s.setSessionActivity(openApp(context))
      session = s
    }

    /** Appelé par le pont (n'importe quel fil) : appliqué sur le fil principal. */
    fun update(context: Context, args: UpdateArgs) {
      val app = context.applicationContext
      main.post { apply(app, args) }
    }

    private fun apply(context: Context, a: UpdateArgs) {
      val s = session(context)
      val prev = state
      state = a
      if (!a.active) {
        s.isActive = false
        Focus.abandon(context)
        instance?.finish()
        return
      }
      if (prev == null || !prev.active || prev.title != a.title || prev.artist != a.artist ||
        prev.cover != a.cover || prev.duration != a.duration) {
        if (a.cover != artFor) {
          art = null
          artFor = a.cover
          loadArt(a.cover)
        }
        s.setMetadata(metadata(a))
      }
      val playback = if (a.playing) PlaybackStateCompat.STATE_PLAYING else PlaybackStateCompat.STATE_PAUSED
      s.setPlaybackState(
        PlaybackStateCompat.Builder()
          .setActions(ACTIONS)
          .setState(playback, (a.position * 1000).toLong(), if (a.playing) 1f else 0f)
          .build(),
      )
      s.isActive = true
      if (a.playing) Focus.request(context)

      val service = instance
      if (service == null) {
        if (!starting) {
          starting = true
          ContextCompat.startForegroundService(context, Intent(context, PlaybackService::class.java))
        }
      } else {
        service.refresh()
      }
    }

    private fun metadata(a: UpdateArgs) = MediaMetadataCompat.Builder()
      .putString(MediaMetadataCompat.METADATA_KEY_TITLE, a.title)
      .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, a.artist)
      .putString(MediaMetadataCompat.METADATA_KEY_ALBUM, a.album)
      .putLong(MediaMetadataCompat.METADATA_KEY_DURATION, (a.duration * 1000).toLong())
      .putBitmap(MediaMetadataCompat.METADATA_KEY_ALBUM_ART, art)
      .build()

    /** Pochette (réseau ou fichier), chargée hors du fil principal. */
    private fun loadArt(cover: String?) {
      if (cover == null) return
      thread(name = "yato-cover") {
        val bitmap = try {
          if (cover.startsWith("file://")) BitmapFactory.decodeFile(cover.removePrefix("file://"))
          else URL(cover).openStream().use { BitmapFactory.decodeStream(it) }
        } catch (_: Exception) {
          null
        } ?: return@thread
        val scaled = if (bitmap.width > 640) Bitmap.createScaledBitmap(bitmap, 640, 640 * bitmap.height / bitmap.width, true) else bitmap
        main.post {
          if (artFor != cover) return@post
          art = scaled
          state?.let { session?.setMetadata(metadata(it)) }
          instance?.refresh()
        }
      }
    }

    /** Priorité audio : pause quand une autre appli prend le son, reprise si c'était passager. */
    private object Focus {
      private var request: AudioFocusRequest? = null
      private var resumeOnGain = false

      fun request(context: Context) {
        if (request != null) return
        val am = context.getSystemService(AudioManager::class.java) ?: return
        val r = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
          .setAudioAttributes(
            AudioAttributes.Builder()
              .setUsage(AudioAttributes.USAGE_MEDIA)
              .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
              .build(),
          )
          // Baisse du volume (GPS…) : le système s'en charge, on continue.
          .setWillPauseWhenDucked(false)
          .setOnAudioFocusChangeListener({ change ->
            when (change) {
              AudioManager.AUDIOFOCUS_LOSS -> {
                resumeOnGain = false
                send("pause")
                abandon(context)
              }
              AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> if (state?.playing == true) {
                resumeOnGain = true
                send("pause")
              }
              AudioManager.AUDIOFOCUS_GAIN -> if (resumeOnGain) {
                resumeOnGain = false
                send("play")
              }
            }
          }, main)
          .build()
        if (am.requestAudioFocus(r) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) request = r
      }

      fun abandon(context: Context) {
        val r = request ?: return
        context.getSystemService(AudioManager::class.java)?.abandonAudioFocusRequest(r)
        request = null
      }
    }
  }

  private var wakeLock: PowerManager.WakeLock? = null
  private var wifiLock: WifiManager.WifiLock? = null

  /** Casque débranché (ou Bluetooth coupé) : pause, comme partout ailleurs sur Android. */
  private val noisy = object : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
      if (intent.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) send("pause")
    }
  }

  override fun onCreate() {
    super.onCreate()
    instance = this
    starting = false
    val nm = getSystemService(NotificationManager::class.java)
    nm.createNotificationChannel(
      NotificationChannel(CHANNEL_ID, getString(R.string.playback_channel), NotificationManager.IMPORTANCE_LOW).apply {
        setShowBadge(false)
      },
    )
    ContextCompat.registerReceiver(this, noisy, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY), ContextCompat.RECEIVER_NOT_EXPORTED)
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    // Boutons du casque et de la notification : ils arrivent ici, puis vont à la session.
    session?.let { MediaButtonReceiver.handleIntent(it, intent) }
    val n = build()
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      startForeground(NOTIFICATION_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
    } else {
      startForeground(NOTIFICATION_ID, n)
    }
    keepAwake(state?.playing == true)
    // Pas de redémarrage automatique : sans le cœur, il n'y aurait rien à jouer.
    return START_NOT_STICKY
  }

  @SuppressLint("MissingPermission") // Notification multimédia : exemptée de la permission.
  fun refresh() {
    getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, build())
    keepAwake(state?.playing == true)
  }

  fun finish() {
    keepAwake(false)
    stopForeground(STOP_FOREGROUND_REMOVE)
    stopSelf()
  }

  private fun keepAwake(on: Boolean) {
    if (on) {
      if (wakeLock == null) {
        wakeLock = getSystemService(PowerManager::class.java)
          .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "555:playback")
          .apply { setReferenceCounted(false); acquire() }
      }
      if (wifiLock == null) {
        @Suppress("DEPRECATION")
        wifiLock = (applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager)
          .createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "555:playback")
          .apply { setReferenceCounted(false); acquire() }
      }
    } else {
      wakeLock?.release()
      wakeLock = null
      wifiLock?.release()
      wifiLock = null
    }
  }

  private fun build(): Notification {
    val a = state
    val playing = a?.playing == true
    val button = { action: Long -> MediaButtonReceiver.buildMediaButtonPendingIntent(this, action) }
    return NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(R.drawable.ic_stat_555)
      .setContentTitle(a?.title)
      .setContentText(a?.artist)
      .setLargeIcon(art)
      .setContentIntent(openApp(this))
      .setDeleteIntent(button(PlaybackStateCompat.ACTION_STOP))
      .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
      .setCategory(NotificationCompat.CATEGORY_TRANSPORT)
      .setOnlyAlertOnce(true)
      .setSilent(true)
      .setOngoing(playing)
      .addAction(android.R.drawable.ic_media_previous, getString(R.string.previous), button(PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS))
      .addAction(
        if (playing) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play,
        getString(if (playing) R.string.pause else R.string.play),
        button(PlaybackStateCompat.ACTION_PLAY_PAUSE),
      )
      .addAction(android.R.drawable.ic_media_next, getString(R.string.next), button(PlaybackStateCompat.ACTION_SKIP_TO_NEXT))
      .setStyle(
        androidx.media.app.NotificationCompat.MediaStyle()
          .setMediaSession(session?.sessionToken)
          .setShowActionsInCompactView(0, 1, 2),
      )
      .build()
  }

  override fun onDestroy() {
    keepAwake(false)
    unregisterReceiver(noisy)
    instance = null
    starting = false
    super.onDestroy()
  }

  override fun onBind(intent: Intent?): IBinder? = null
}
