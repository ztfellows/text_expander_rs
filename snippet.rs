
    thread::scope(|s| {
        s.spawn(|| {
            let callback = |event: Event| {
                if let Ok(mut buffer) = key_buffer.lock() {
                    handle_key_press(event, &mut buffer);
                }
            };

            println!("Listening for keyboard events...");
            //listen(callback).expect("Failed to listen for events");
        });
    });

    