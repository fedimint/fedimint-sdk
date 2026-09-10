# JNA and the UniFFI-generated bindings resolve types reflectively, so R8 must
# not rename or strip them in the consuming app.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class org.fedimint.sdk.** { *; }
