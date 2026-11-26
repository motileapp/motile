package app.motile.core

import android.app.Application
import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** In the sandbox the canvas and program tests load the core in: a library loads in one class loader only. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], application = Application::class)
class PushOpeningTest {
    @Test
    fun theCoreOpensAPushSealedToTheDeviceKeyOnDisk() {
        val dir = Files.createTempDirectory("motile-push").toFile()
        File(dir, "device.key").writeText(DEVICE_KEY)
        val opened = Pushed.of(Native.openPush(dir.path, SEALED_ALERT)) as Pushed.Alert
        assertEquals("Fix the login", opened.alert.threadTitle)
        assertEquals("Needs your approval · Run greet.py", opened.alert.body)
        val work = Pushed.of(Native.openPush(dir.path, SEALED_WORK)) as Pushed.Work
        assertEquals(3, work.work.working)
        assertEquals(WorkState.Waiting, work.work.items.single().state)
        assertEquals("", Native.openPush(dir.path, "AAAA"))
        dir.deleteRecursively()
    }

    private companion object {
        /** A device key, and what crates/protocol's `seal_pushed` sealed to it. */
        const val DEVICE_KEY = "0101010101010101010101010101010101010101010101010101010101010101"
        const val SEALED_ALERT =
            "wH9RcRXsr5hvABvF/rKTOa2WmZIfqhXeEeS/of/snV+y5Mo8Pr09WOxEBXydEnhAMSDpPIQFKpu3urs1tYxDS6ifN5rc8tS68qSu6ekgLtsCdH4jfsVdV1/YhtpB21tQC0NMF31XGXaQV69SEbontW7SUkpFUw80do5p5dtbcfbCa/d5K5Rrlp3jOPwzLjSYexSnkn1qGTf1ecpUcANMs/aMLse/bCN6wQStoaaSISegOtxs7kIcUO3o3pXsKDyov47eexyBVas7f+EUJwEsxu/NfPvyw09tDzJ+stPEBHH36PtYfYUcZV1137x55UKrSvd4Jm7bbxP5UsVVbbew440btKeNiaM="
        const val SEALED_WORK =
            "SolzKnriXaRoDJxkYio2SbRK0WhwbQ7b3bcDuHoueSTWRrwH4z4CT7Z4wJre1U+tEzXKKXnE4WCqojceXZGqPlG1Al/Zrj8LNyGNVCvubYZmg9QwOIdcBSZ6yQySTj3L+35Z8U+uWv7MKMYcouMntb6UkgVOC+PskXXsbtdownt9osmlYE+meEXhkio2RhGboglycJBDs8RJSnB6Ce+UsAvlypVE4xAvFOjc3UeYFBWZfY7IYpLVUTqZUpeJ3pQZGl92RATNk0NoHf0kTMdenkPfJpkZahve0TBJpc/EKsfjxjEhbVqDfJwrHb0G0BQ7vQYgg2RT4SwIYr+0XzlHzA=="
    }
}
