package me.leoank.saarthi

import android.content.SharedPreferences
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

/** One hub the phone can connect to. Reached over the phone's own network (the Tailscale
 *  app) for now; a tailnet identity of the app's own per hub comes with the phone's tsnet
 *  (ADR-0016), and its settings will live here. */
data class HubProfile(
    val id: String = UUID.randomUUID().toString(),
    val name: String,
    val address: String,
    val token: String,
) {
    /** The address's host, for a hub without a name and for resolving. */
    val host: String
        get() = address.trim().removePrefix("http://").removePrefix("https://").substringBefore('/').substringBefore(':')

    val title: String
        get() = name.ifBlank { host.ifBlank { "hub" } }
}

/** The hubs and the active one, in the app's preferences. The active hub's address and
 *  token are mirrored under the keys the app used before it had a list, which the
 *  notification actions read to connect without the UI. A phone with only those keys
 *  gets them as its one hub. */
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
                HubProfile(o.getString("id"), o.optString("name"), o.getString("address"), o.getString("token"))
            }
        }.getOrDefault(emptyList())
    }

    fun active(hubs: List<HubProfile>): String? =
        prefs.getString(ACTIVE, null)?.takeIf { id -> hubs.any { it.id == id } } ?: hubs.firstOrNull()?.id

    fun save(hubs: List<HubProfile>, active: String?) {
        val arr = JSONArray()
        for (h in hubs) arr.put(JSONObject().put("id", h.id).put("name", h.name).put("address", h.address).put("token", h.token))
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
