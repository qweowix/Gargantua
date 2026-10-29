# JNI: Core.call ищется по имени из Rust
-keep class app.blackhole.Core { native <methods>; }
# NewPipe: Rhino (расшифровка подписей YouTube) и jsoup лезут в себя рефлексией
-keep class org.schabi.newpipe.extractor.** { *; }
-keep class org.mozilla.javascript.** { *; }
-keep class org.mozilla.classfile.ClassFileWriter
-dontwarn org.mozilla.javascript.**
-dontwarn javax.script.**
-dontwarn java.beans.**
-dontwarn jdk.dynalink.**
-dontwarn com.google.re2j.**
-dontwarn org.jsoup.**
# kotlinx.serialization
-keepattributes *Annotation*, InnerClasses
-keepclassmembers class app.blackhole.** { *** Companion; kotlinx.serialization.KSerializer serializer(...); }
-keep,includedescriptorclasses class app.blackhole.**$$serializer { *; }
