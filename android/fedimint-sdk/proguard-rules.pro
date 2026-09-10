# Applied when building this library itself. The rules consumers need are in
# consumer-rules.pro, which is packaged into the AAR.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class org.fedimint.sdk.** { *; }
