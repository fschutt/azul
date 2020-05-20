{
            unsafe { crate::callbacks::CALLBACK = callback };
            az_app_new(data.leak(), config.leak(), crate::callbacks::translate_callback)
        }