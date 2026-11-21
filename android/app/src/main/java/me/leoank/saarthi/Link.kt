package me.leoank.saarthi

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import androidx.core.app.ServiceCompat
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.saarthi_core.NotificationListener

/** Notifications held from the hub itself (ADR-0075): a foreground service keeps the hub's
 *  `Notifications` stream open, over the app's own tailnet identity where the hub has one,
 *  and shows each one as a push would; while it holds the stream the hub sends nothing to
 *  this phone's UnifiedPush endpoint, which stays as the fallback. It opens the stream
 *  again whenever it drops, at once on a new network, and shows itself as a silent
 *  notification of the lowest importance. */
class LinkService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var holding: Job? = null
    /** A nudge to open the stream again now: a new network came up. */
    private val retry = Channel<Unit>(Channel.CONFLATED)
    private val network = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            retry.trySend(Unit)
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        getSystemService(ConnectivityManager::class.java).registerDefaultNetworkCallback(network)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // Started as a foreground service, it must come to the foreground before anything,
        // even to stop at once.
        ServiceCompat.startForeground(
            this, NOTIFICATION, status("Connecting…"),
            if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING else 0,
        )
        if (intent?.action == STOP || !enabled(this) || HubStore(getSharedPreferences("saarthi", 0)).current() == null) {
            holding?.cancel()
            ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
            stopSelf()
            return START_NOT_STICKY
        }
        // Started again (a hub switched to): hold the stream of the hub now active.
        holding?.cancel()
        holding = scope.launch { hold() }
        return START_STICKY
    }

    override fun onDestroy() {
        runCatching { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(network) }
        scope.cancel()
        super.onDestroy()
    }

    /** Opens the stream and keeps it open: again after each drop, waiting longer each time
     *  up to a minute, or at once when the network changes. */
    private suspend fun hold() {
        var wait = 2_000L
        while (scope.isActive) {
            val name = HubStore(getSharedPreferences("saarthi", 0)).current()?.title ?: "the hub"
            val ended = CompletableDeferred<String>()
            var beat = System.currentTimeMillis()
            try {
                val hub = Push.hub(this)
                val sub = hub.notifications(object : NotificationListener {
                    override fun onNotification(pushJson: String) {
                        beat = System.currentTimeMillis()
                        if (!Push.inFront) Push.show(this@LinkService, pushJson)
                    }
                    override fun onBeat() {
                        beat = System.currentTimeMillis()
                    }
                    override fun onError(message: String) {
                        ended.complete(message)
                    }
                })
                notify("Holding the link to $name")
                wait = 2_000L
                // Held until it ends, or goes quiet past two missed beats.
                var why: String? = null
                while (why == null) {
                    why = withTimeoutOrNull(30_000) { ended.await() }
                    if (why == null && System.currentTimeMillis() - beat > SILENT_MS) why = "no word from the hub"
                }
                sub.cancel()
                Log.i(TAG, "the notifications stream ended: $why")
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.i(TAG, "the notifications stream could not open: ${e.message}")
            }
            notify("Reconnecting to $name…")
            withTimeoutOrNull(wait) { retry.receive() }
            wait = (wait * 2).coerceAtMost(60_000L)
        }
    }

    private fun notify(text: String) {
        getSystemService(NotificationManager::class.java).notify(NOTIFICATION, status(text))
    }

    /** The service's own notification: silent, of the lowest importance, opening the app. */
    private fun status(text: String): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Link to the hub", NotificationManager.IMPORTANCE_MIN).apply {
                description = "Shown while saarthi holds the hub's notifications open"
                setShowBadge(false)
            },
        )
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentText(text)
            .setContentIntent(open)
            .setOngoing(true)
            .build()
    }

    /** Started again at boot, when it was on. */
    class Boot : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action == Intent.ACTION_BOOT_COMPLETED) start(context)
        }
    }

    companion object {
        private const val TAG = "saarthi.link"
        private const val CHANNEL = "link"
        private const val NOTIFICATION = 7700
        private const val STOP = "me.leoank.saarthi.link.stop"
        /** The hub beats every 45 s; two missed is a dead stream. */
        private const val SILENT_MS = 100_000L

        /** Whether notifications come over a held link: on unless turned off in Settings. */
        fun enabled(context: Context) = context.getSharedPreferences("saarthi", 0).getBoolean("link", true)

        fun setEnabled(context: Context, on: Boolean) {
            context.getSharedPreferences("saarthi", 0).edit().putBoolean("link", on).apply()
            if (on) start(context) else context.startService(Intent(context, LinkService::class.java).setAction(STOP))
        }

        /** Starts holding the link, or holds it again to the hub now active. */
        fun start(context: Context) {
            if (!enabled(context)) return
            runCatching { context.startForegroundService(Intent(context, LinkService::class.java)) }
                .onFailure { Log.w(TAG, "the link could not start", it) }
        }

        /** Whether Android lets the app keep its link through Doze. */
        fun unrestricted(context: Context) =
            context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName)
    }
}
