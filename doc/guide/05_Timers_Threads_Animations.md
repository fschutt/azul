# Timers, Threads & Animations

This page mainly concerns itself with offloading callbacks to background threads,
for example in order to keep the UI responsive while waiting for a large file to load.

## Timers

If you've ever had to develop a website with JavaScript, you might be familiar 
with the `setInterval` and `clearInterval` functions, which execute a callback 
every X milliseconds. Azul features a similar concept called "timers",
but takes a different approach than JavaScript.

A `Timer` is simply a callback function that is run in the event loop (on the main thread). 
A `Timer` has full mutable access to the data model and is equipped with:

- An optional timeout duration (maximum duration that the daemon should run)
- An interval (how fast the timer should ring, i.e.: run only every 2 seconds)
- A delay (if the timer should only start after a certain time)

The `TimerCallback` returns two values: A `TerminateTimer` that determines
whether the timer should terminate itself (for example if the timer should
run only once, you can set this to `Terminate`, so that the timer is removed
after it's been called once) and an `UpdateScreen`, which is used to determine
whether the `layout()` function needs to be called again.

Additionally, timers have a `TimerId`, which can be used to identify instances 
of a timer running, i.e. to check if a timer (with the same ID) is already running.
If you try to add a timer, but a timer with the same ID is already running, nothing
will happen (the second timer won't get added). This is done to prevent timers
from executing twice, i.e. if a user clicks the button twice, the second
`.add_timer()` will automatically be ignored.

The following example shows a timer that updates the `state.stopwatch_time` field
based on the `state.stopwatch_start`:

```rust
fn timer_callback(state: &mut MyDataModel, _: &mut AppResources) -> (UpdateScreen, TerminateTimer) {
    state.stopwatch_time = Instant::now() - state.stopwatch_start;
    (Redraw, TerminateTimer::Continue)
}
```

This timer, by itself, will count up towards infinity, but we can limit the timer to 
terminate itself after 5 seconds:

```rust
fn on_start_timer_btn_clicked(app_state: &mut AppState<MyDataModel>, _: &mut CallbackInfo<MyDataModel>) 
-> UpdateScreen 
{
    app_state.modify(|state| state.stopwatch_start = Instant::now());
    let timer = Timer::new(timer_callback).with_timeout(Duration::from_secs(5));
    app_state.add_timer(TimerId::new(), timer);
    Redraw
}
```

Now, in the `layout()` function, we can format our timer nicely:

```rust
impl Layout for TimerApplication {
    fn layout(&self, _: LayoutInfo<Self>) -> Dom<Self> {

        let sec_total = self.stopwatch_time.as_secs();
        let min = sec_total / 60;
        let secs = sec_total % 60;
        let ms = self.stopwatch_time.subsec_millis();

        let current_time = Label::new(format!("{:02}:{:02}:{:02}", min, sec, ms)).dom();
        let start_button = Button::with_label("Start timer").dom()
                               .with_callback(On::Click, Callback(on_start_timer_btn_clicked));
        
        Dom::div()
            .with_child(current_time)
            .with_child(start_button)
    }
}
```

Here we have a fully featured stopwatch - it's that easy. If the button is
clicked, a timer will start and automatically terminate itself after 5 seconds.
It will update the screen at the fastest possible framerate (since we haven't limited 
the interval of the timer yet), so that might be something to improve.

Timers are always run on the main thread, before any callbacks are run - they 
shouldn't be used for IO or heavy calculation, rather they should be used as timers
or as things that should check / poll for something every X seconds.

## Threads

A `Thread` is a slight abstraction in order to easily offload calculations to a
separate thread. They take a pure function (a function which has a signature of `U -> T`)
and run it in a new thread (as a slight abstraction over `std::thread`).

You must call `.await()` on the thread, otherwise it will panic if it goes out of scope
while the thread is still running. As an example:

```rust
fn pure_function(input: usize) -> usize { input + 1 }

let thread_1 = Thread::new(5, pure_function);
let thread_2 = Thread::new(10, pure_function);
let thread_3 = Thread::new(20, pure_function);

// thread_1, thread_2 and thread_3 run in parallel here...

let result_1 = thread_1.await();
let result_2 = thread_2.await();
let result_3 = thread_3.await();

assert_eq!(result_1, Ok(6));
assert_eq!(result_2, Ok(11));
assert_eq!(result_3, Ok(21));
```

## Tasks

A `Task` is more or less the same as a thread, but handled by the framework.
For example, you might want to wait for a file to load or a database connection
to be established while still keeping the UI responsive. For this purpose you
can create a `Task` (which creates a `std::thread` internally) and hand that 
`Task` to Azul, which will then automatically join it after the thread has
finished running (so joining the thread won't block the UI).

A `TaskCallback` takes two arguments: an `Arc<Mutex<T>>` and a `DropCheck` type - the latter
is necessary so that Azul can determine if the thread has finished executing (when the
`DropCheck` is dropped, Azul will try to join the thread). Please simply ignore this argument
and don't use it inside the callback.

```rust
fn do_something_async(app_data: Arc<Mutex<DataModel>>, _: DropCheck) {
    thread::sleep(Duration::from_secs(10));
    app_data.modify(|state| state.is_thread_finished = true);
}

fn start_thread_on_btn_click(app_state: &mut AppState<MyDataModel>, _: &mut CallbackInfo<MyDataModel>) 
-> UpdateScreen 
{
     // data.clone() only clones the Arc, not a deep copy
     app_state.add_task(Task::new(self.data.clone(), connect_to_db_async));
     Redraw
}
```

A note: If you'd put the `thread::sleep` inside of the `.modify` closure, you'd block the main
thread from redrawing the UI. So please only use `modify` and lock the data wnhen absolutely
necessary, i.e. when reading or writing data from / into the application data model. Also, after a
task is finished, the UI will always redraw itself, this should be configurable in the future
and is a work in progress.

A `Task` can (optionally) have a `Timer` attached to it (via `.then()`) - this defines a timer
that is run immediately after the `Task` has ended. This is useful when working with non-threadsafe
data, where some part of that data can be loaded on a background thread, but can't 
be prepared for a UI visualization from a non-main thread.

## Summary

In this chapter you've learned how to use timers, async IO and handle thread-safe and non-thread safe data
within your data model.

<br/>
<br/>

<a href="$$ROOT_RELATIVE$$/guide">Back to overview</a>
