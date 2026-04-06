package app.tauri.credentials

import android.app.Activity
import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKeys
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class StoreArgs {
    lateinit var domain: String
    lateinit var username: String
    lateinit var password: String
}

@InvokeArg
class DomainArgs {
    lateinit var domain: String
}

/// Credential storage plugin using Android EncryptedSharedPreferences (backed by Android Keystore).
@TauriPlugin
class CredentialPlugin(private val activity: Activity) : Plugin(activity) {

    private fun getPrefs(): SharedPreferences {
        val masterKeyAlias = MasterKeys.getOrCreate(MasterKeys.AES256_GCM_SPEC)
        return EncryptedSharedPreferences.create(
            "onyx_credentials",
            masterKeyAlias,
            activity,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
        )
    }

    @Command
    fun store(invoke: Invoke) {
        val args = invoke.parseArgs(StoreArgs::class.java)
        try {
            getPrefs().edit()
                .putString("${args.domain}::username", args.username)
                .putString("${args.domain}::${args.username}::password", args.password)
                .apply()
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("Failed to store credentials: ${e.message}")
        }
    }

    @Command
    fun load(invoke: Invoke) {
        val args = invoke.parseArgs(DomainArgs::class.java)
        try {
            val prefs = getPrefs()
            val username = prefs.getString("${args.domain}::username", null)
            if (username == null) {
                invoke.reject("No credentials found for '${args.domain}'. Run setup or configure environment variables.")
                return
            }
            val password = prefs.getString("${args.domain}::${username}::password", null)
            if (password == null) {
                invoke.reject("No password found for '${args.domain}' user '$username'")
                return
            }
            val result = JSObject()
            result.put("username", username)
            result.put("password", password)
            invoke.resolve(result)
        } catch (e: Exception) {
            invoke.reject("Failed to load credentials: ${e.message}")
        }
    }

    @Command
    fun delete(invoke: Invoke) {
        val args = invoke.parseArgs(DomainArgs::class.java)
        try {
            val prefs = getPrefs()
            val username = prefs.getString("${args.domain}::username", null)
            val editor = prefs.edit().remove("${args.domain}::username")
            if (username != null) {
                editor.remove("${args.domain}::${username}::password")
            }
            editor.apply()
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("Failed to delete credentials: ${e.message}")
        }
    }
}
