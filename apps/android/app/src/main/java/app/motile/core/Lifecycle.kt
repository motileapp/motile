package app.motile.core

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.Handler
import android.os.Looper
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner

/**
 * Tells the core when the client comes to the front or changes networks, so that it dials again
 * the moment the client is back instead of waiting to find out.
 */
class Lifecycle {
    private val main = Handler(Looper.getMainLooper())
    private var path: String? = null

    fun start(context: Context, store: AppStore) {
        ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStart(owner: LifecycleOwner) {
                store.core.send("foreground")
                store.becameActive()
            }
        })
        val connectivity = context.getSystemService(ConnectivityManager::class.java)
        connectivity.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
            override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
                val name = when {
                    capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
                    capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
                    capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
                    else -> "up"
                }
                main.post { pathChanged(name, store) }
            }

            override fun onLost(network: Network) {
                main.post { pathChanged("down", store) }
            }
        })
    }

    private fun pathChanged(name: String, store: AppStore) {
        val before = path
        path = name
        if (before == null || before == name) return
        store.core.send("network_changed")
    }
}
