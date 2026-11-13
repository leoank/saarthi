package me.leoank.saarthi

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import uniffi.saarthi_core.Hub
import uniffi.saarthi_core.TailnetLogin
import java.io.File
import java.security.KeyStore
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** One hub the phone can connect to. With a tailnet auth key the app reaches it through an
 *  identity of its own on the hub's tailnet (ADR-0016); without one, over the phone's own
 *  network (the Tailscale app). */
data class HubProfile(
    val id: String = UUID.randomUUID().toString(),
    val name: String,
    val address: String,
    val token: String,
    /** The tailnet auth key; kept on the phone, encrypted, so the app can log in again. */
    val tailnetKey: String = "",
    /** This phone's device name on the hub's tailnet. */
    val tailnetHost: String = "",
) {
    /** The address's host, for a hub without a name and for resolving. */
    val host: String
        get() = address.trim().removePrefix("http://").removePrefix("https://").substringBefore('/').substringBefore(':')

    val title: String
        get() = name.ifBlank { host.ifBlank { "hub" } }

    val viaTailnet: Boolean
        get() = tailnetKey.isNotBlank()

    /** Where this hub's tailnet identity keeps its state. */
    fun tailnetDir(context: Context): File = File(context.filesDir, "tailnet/$id")

    /** This phone's device name on the hub's tailnet. */
    val tailnetName: String
        get() = tailnetHost.ifBlank { defaultTailnetHost() }

    fun tailnetLogin(context: Context) = TailnetLogin(tailnetDir(context).path, tailnetName, tailnetKey.trim())

    /** Opens the hub, through the tailnet identity when the hub has one. */
    suspend fun connect(context: Context): Hub =
        if (viaTailnet) Hub.connectViaTailnet(address.trim(), token.trim(), tailnetLogin(context))
        else Hub.connect(address.trim(), token.trim())

    companion object {
        /** `saarthi-` and the phone's model, as a tailnet machine name. */
        fun defaultTailnetHost(): String =
            "saarthi-" + Build.MODEL.lowercase().replace(Regex("[^a-z0-9]+"), "-").trim('-')
    }
}

/** The hubs and the active one, in the app's preferences. The active hub's address and
 *  token are mirrored under the keys the app used before it had a list. A phone with only
 *  those keys gets them as its one hub. Tailnet keys are stored encrypted under a key in
 *  the Android Keystore, which never leaves it. */
class HubStore(private val prefs: SharedPreferences) {
    fun load(): List<HubProfile> {
        val json = prefs.getString(HUBS, null)
        if (json == null) {
            val address = prefs.getString(ADDRESS, null)?.trim().orEmpty()
            val token = prefs.getString(TOKEN, null)?.trim().orEmpty()
            if (address.isEmpty() || token.isEmpty()) return emptyList()
            val only = HubProfile(name = "", address = address, token = token)
            save(listOf(only), only.id)
            return listOf(only)
        }
        return runCatching {
            val arr = JSONArray(json)
            (0 until arr.length()).map { k ->
                val o = arr.getJSONObject(k)
                HubProfile(
                    o.getString("id"), o.optString("name"), o.getString("address"), o.getString("token"),
                    tailnetKey = o.optString("tailnet_key").takeIf { it.isNotEmpty() }?.let { Sealed.open(it) }.orEmpty(),
                    tailnetHost = o.optString("tailnet_host"),
                )
            }
        }.getOrDefault(emptyList())
    }

    /** The active hub, as the notification actions find it. */
    fun current(): HubProfile? = load().let { hubs -> hubs.firstOrNull { it.id == active(hubs) } }

    fun active(hubs: List<HubProfile>): String? =
        prefs.getString(ACTIVE, null)?.takeIf { id -> hubs.any { it.id == id } } ?: hubs.firstOrNull()?.id

    fun save(hubs: List<HubProfile>, active: String?) {
        val arr = JSONArray()
        for (h in hubs) {
            val o = JSONObject().put("id", h.id).put("name", h.name).put("address", h.address).put("token", h.token)
            if (h.viaTailnet) o.put("tailnet_key", Sealed.seal(h.tailnetKey)).put("tailnet_host", h.tailnetHost)
            arr.put(o)
        }
        val current = hubs.firstOrNull { it.id == active }
        prefs.edit()
            .putString(HUBS, arr.toString())
            .putString(ACTIVE, current?.id)
            .putString(ADDRESS, current?.address)
            .putString(TOKEN, current?.token)
            .apply()
    }

    private companion object {
        const val HUBS = "hubs"
        const val ACTIVE = "active_hub"
        const val ADDRESS = "address"
        const val TOKEN = "token"
    }
}

/** AES-GCM under a key kept in the Android Keystore: the stored text is the IV and the
 *  ciphertext, in base64. */
private object Sealed {
    private const val ALIAS = "saarthi-hub-secrets"
    private const val CIPHER = "AES/GCM/NoPadding"

    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .build()
        )
        return gen.generateKey()
    }

    fun seal(plain: String): String {
        val c = Cipher.getInstance(CIPHER).apply { init(Cipher.ENCRYPT_MODE, key()) }
        return Base64.encodeToString(c.iv + c.doFinal(plain.toByteArray()), Base64.NO_WRAP)
    }

    /** The plain text, or empty when it cannot be opened (the Keystore key is gone, as
     *  after a restore onto another phone): the hub then needs its key again. */
    fun open(sealed: String): String = runCatching {
        val bytes = Base64.decode(sealed, Base64.NO_WRAP)
        val c = Cipher.getInstance(CIPHER).apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes, 0, 12)) }
        String(c.doFinal(bytes, 12, bytes.size - 12))
    }.getOrDefault("")
}
