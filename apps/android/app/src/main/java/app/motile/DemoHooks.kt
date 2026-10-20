package app.motile

/**
 * What the demo driver asks of views it can't reach through the store: each view that can do it
 * sets its hook while it is shown.
 */
object DemoHooks {
    /** Gives the composer the keyboard. */
    var focusComposer: (() -> Unit)? = null
    /** Types in the composer where its cursor is, as a keyboard does. */
    var typeInComposer: ((String) -> Unit)? = null
    /** Scrolls the transcript to its start. */
    var scrollTranscriptToTop: (() -> Unit)? = null
    /** Gives the front search field the keyboard, and types the text in it. */
    var search: ((String) -> Unit)? = null
    /** Where the nodes of each program were laid out, by its row, while the driver runs. */
    var programFrames: MutableMap<String, app.motile.transcript.openui.UiFrames>? = null
    /** Touches the window as a finger does. */
    var touch: ((android.view.MotionEvent) -> Unit)? = null
}
