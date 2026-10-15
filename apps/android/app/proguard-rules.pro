# The core calls these through JNI, which the shrinker can't see.
-keep class app.motile.core.Native { *; }
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
