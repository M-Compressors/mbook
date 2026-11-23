//! The Android shell of the mbook application.
//!
//! The activity of the APK, `MbookActivity` (`android/java`), is the
//! `GpuiInputActivity` of gpui-mobile, a `NativeActivity`: it loads this
//! library, and `android-activity` calls `android_main` on a thread of its
//! own. The shell does what the
//! desktop `main` does: it chooses the allocator, asks the platform where
//! files are kept, and hands over to `mbook_ui::run`. On any other target the
//! library is empty.

#[cfg(target_os = "android")]
mod android {
    use gpui_kit::Application;
    use gpui_mobile::android::jni;
    use mbook_ui::Roots;

    #[global_allocator]
    static GLOBAL: mimalloc3::MiMalloc = mimalloc3::MiMalloc;

    /// The folder of the library inside the files of the application.
    const DATA: &str = "mbook";
    /// The folder of the cache beside it: `android-activity` names no cache
    /// directory, and the files of the application are private to it.
    const CACHE: &str = "cache";

    /// Starts the application; returns when the activity finishes.
    ///
    /// `android-activity` looks the function up by its unmangled name.
    #[expect(
        unsafe_code,
        reason = "android-activity calls the entry point by its unmangled name"
    )]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "android-activity declares the entry point with the app by value"
    )]
    #[unsafe(no_mangle)]
    extern "Rust" fn android_main(app: android_activity::AndroidApp) {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("mbook"),
        );
        // A panic reaches logcat instead of ending the process silently.
        jni::install_panic_hook();
        let Some(files) = app.internal_data_path() else {
            log::error!("the activity has no folder for its files");
            return;
        };
        jni::init_platform(&app);
        let Some(platform) = jni::shared_platform() else {
            log::error!("the GPUI platform of Android could not be created");
            return;
        };
        let roots = Roots::new(files.join(DATA), files.join(CACHE));
        mbook_ui::run(
            Application::with_platform(platform.into_rc()),
            roots,
            Vec::new(),
        );
    }
}
