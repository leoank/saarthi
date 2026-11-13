package me.leoank.saarthi

import android.Manifest
import android.app.Activity
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.json.JSONObject
import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.PushService
import org.unifiedpush.android.connector.UnifiedPush
import org.unifiedpush.android.connector.data.PushEndpoint
import org.unifiedpush.android.connector.data.PushMessage
import uniffi.saarthi_core.Hub

/** Notifications through UnifiedPush (ADR-0026): the ntfy app on the phone is the
 *  distributor; saarthi registers with it, gives the endpoint to the hub, and shows each
 *  message as its own notification, with Allow and Deny on a permission request. */
object Push {
    private const val TAG = "saarthi.push"
    private const val CHANNEL = "waiting"
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    /** After connecting: ask for the notification permission and register with the
     *  phone's distributor; the endpoint arrives in [Receiver.onNewEndpoint]. */
    fun start(activity: Activity) {
        if (Build.VERSION.SDK_INT >= 33 &&
            activity.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            activity.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        }
        UnifiedPush.tryUseCurrentOrDefaultDistributor(activity) { found ->
            if (found) UnifiedPush.register(activity) else Log.w(TAG, "no UnifiedPush distributor on this phone")
        }
    }

    /** Opens a connection to the active hub, for work outside the UI. */
    suspend fun hub(context: Context): Hub {
        val hub = HubStore(context.getSharedPreferences("saarthi", 0)).current() ?: error("no hub is set up")
        return hub.connect(context)
    }

    fun launch(block: suspend () -> Unit) = scope.launch {
        try {
            block()
        } catch (e: Exception) {
            Log.w(TAG, "push work failed", e)
        }
    }

    fun show(context: Context, json: String) {
        val m = JSONObject(json)
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Waiting for you", NotificationManager.IMPORTANCE_HIGH)
        )
        // A usage alert has no session; its key tells it apart.
        val id = (m.optString("pending_id").ifEmpty { m.optString("session_id") }.ifEmpty { m.optString("key") }).hashCode()
        val open = PendingIntent.getActivity(
            context, id,
            Intent(context, MainActivity::class.java)
                .putExtra("node_id", m.optString("node_id"))
                .putExtra("session_id", m.optString("session_id"))
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val builder = android.app.Notification.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(m.optString("title"))
            .setContentText(m.optString("body"))
            .setStyle(android.app.Notification.BigTextStyle().bigText(m.optString("body")))
            .setContentIntent(open)
            .setAutoCancel(true)
        if (m.optString("kind") == "permission") {
            for (decision in listOf("allow", "deny")) {
                val intent = Intent(context, ActionReceiver::class.java)
                    .setAction("me.leoank.saarthi.$decision")
                    .putExtra("node_id", m.getString("node_id"))
                    .putExtra("pending_id", m.getString("pending_id"))
                    .putExtra("notification", id)
                val action = PendingIntent.getBroadcast(
                    context, id * 2 + decision.length, intent,
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                )
                val label = decision.replaceFirstChar { it.uppercase() }
                builder.addAction(android.app.Notification.Action.Builder(null, label, action).build())
            }
        }
        manager.notify(id, builder.build())
    }

    class Receiver : PushService() {
        override fun onNewEndpoint(endpoint: PushEndpoint, instance: String) {
            val prefs = getSharedPreferences("saarthi", 0)
            prefs.edit().putString("push_endpoint", endpoint.url).apply()
            launch { hub(this).registerPush(endpoint.url) }
        }

        override fun onMessage(message: PushMessage, instance: String) {
            show(this, String(message.content))
        }

        override fun onRegistrationFailed(reason: FailedReason, instance: String) {
            Log.w(TAG, "UnifiedPush registration failed: $reason")
        }

        override fun onUnregistered(instance: String) {
            val prefs = getSharedPreferences("saarthi", 0)
            val endpoint = prefs.getString("push_endpoint", null) ?: return
            prefs.edit().remove("push_endpoint").apply()
            launch { hub(this).unregisterPush(endpoint) }
        }
    }

    /** Allow or Deny tapped on a notification. */
    class ActionReceiver : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            val nodeId = intent.getStringExtra("node_id") ?: return
            val pendingId = intent.getStringExtra("pending_id") ?: return
            val done = goAsync()
            val app = context.applicationContext
            launch {
                try {
                    val h = hub(app)
                    if (intent.action?.endsWith("allow") == true) h.allow(nodeId, pendingId)
                    else h.deny(nodeId, pendingId, "Denied from the saarthi notification")
                    app.getSystemService(NotificationManager::class.java)
                        .cancel(intent.getIntExtra("notification", 0))
                } finally {
                    done.finish()
                }
            }
        }
    }
}
