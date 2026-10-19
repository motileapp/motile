package app.motile.core

import android.content.Context

/** The Rust core's JNI functions (apps/android/core). Everything is JSON text. */
object Native {
    init {
        System.loadLibrary("motile_android")
    }

    /** Starts the core with an `api::Config`. False if it couldn't start. */
    @JvmStatic external fun start(context: Context, config: String): Boolean

    /** Sends one command, an `api::Envelope`. The answer arrives as a `reply` event. */
    @JvmStatic external fun send(command: String)

    /** Waits for the next event. */
    @JvmStatic external fun next(): String

    /** Lays out a canvas component, a `canvas::Canvas`, into its `canvas::Scene`. Any thread. */
    @JvmStatic external fun uiLayout(input: String): String

    /** Builds the page an HtmlView or a YouTube shows, a `web::PageRequest`, into its `web::Page`. Any thread. */
    @JvmStatic external fun uiPage(input: String): String
}
