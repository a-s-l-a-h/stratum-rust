use std::sync::atomic::{AtomicI32, Ordering};
use stratum::android::widget::{Button, LinearLayout, TextView};
use stratum::*;

stratum_main!(|| {
    on_create(|act: Activity| {
        let root = LinearLayout::new(&act).unwrap();
        root.setOrientation(1).unwrap(); // VERTICAL
        root.setPadding(48, 48, 48, 48).unwrap();

        let title = TextView::new(&act).unwrap();
        title.setText("Hello from Stratum Rust").unwrap();
        title.setTextSize(22.0).unwrap();
        root.addView(&title).unwrap();

        let btn = Button::new(&act).unwrap();
        btn.setText("Tap me").unwrap();
        let label = title.clone();
        let n = AtomicI32::new(0);
        btn.setOnClickListener(move |_: Args| {
            let c = n.fetch_add(1, Ordering::SeqCst) + 1;
            label.setText(format!("Clicked {} times", c)).unwrap();
        })
        .unwrap();
        root.addView(&btn).unwrap();

        set_content_view(&root);
    });
});