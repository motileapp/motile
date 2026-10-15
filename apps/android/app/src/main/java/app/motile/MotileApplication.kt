package app.motile

import android.app.Application
import app.motile.core.AppStore
import app.motile.core.Appearance
import app.motile.core.Preferences
import app.motile.platform.Platform

/** The process: the store lives as long as it does, whatever the activity goes through. */
class MotileApplication : Application() {
    lateinit var store: AppStore
        private set

    override fun onCreate() {
        super.onCreate()
        Platform.start(this)
        Preferences.start(this)
        Appearance.load()
        store = AppStore(this)
    }
}
