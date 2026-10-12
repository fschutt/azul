# AzMonitor's words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## The window

azmonitor-about-summary = What uses the CPU, the memory, the disk and the network - and a way to stop it. Readings once a second on a background thread, a minute of history, and a window that never lays itself out again to show them. Part of the Azlin apps, built with azul.
azmonitor-category-monitor = Monitor
azmonitor-tab-processes = Processes
azmonitor-tab-performance = Performance
azmonitor-tab-networking = Networking
azmonitor-tab-users = Users

## Processes

azmonitor-col-name = Image Name
azmonitor-col-pid = PID
azmonitor-col-user = User Name
azmonitor-col-cpu = CPU
azmonitor-col-memory = Memory
azmonitor-col-disk = Disk
azmonitor-col-status = Status
azmonitor-col-description = Description
azmonitor-end-process = End Process
azmonitor-select-first = Select a process first
azmonitor-filter = Filter by name, user or PID
azmonitor-filter-processes = Filter processes
azmonitor-reading-system = Reading the system…
azmonitor-waiting-sample = The sample machine (--sample).
azmonitor-waiting-live = Processes, CPU, memory, disks and networks, on a background thread.
azmonitor-process-of-user = { $name } ({ $pid }), user { $user }
azmonitor-end-detail = Unsaved work in it is lost. Kill ends it without asking it to quit.
azmonitor-end-needs-admin = It belongs to { $user }: ending it needs administrator rights.
azmonitor-end-ask = End { $name }?
azmonitor-end-button = End process
azmonitor-kill = Kill
azmonitor-killed = Killed { $name } ({ $pid })
azmonitor-ended = Ended { $name } ({ $pid })
azmonitor-not-ended = { $name } ({ $pid }) could not be ended: it belongs to another user (administrator rights are needed) or it has just quit.
azmonitor-cannot-end = { $name } ({ $pid }) cannot be ended on this system.
azmonitor-needs-admin = Ending { $name } ({ $pid }) needs administrator rights.
azmonitor-no-process = No process { $pid } is running.

## Performance

azmonitor-cpu-usage = CPU Usage
azmonitor-cpu-history = CPU Usage History
azmonitor-cpu-history-of = CPU Usage History - { $cpu }
azmonitor-memory = Memory
azmonitor-memory-history = Physical Memory Usage History
azmonitor-totals = Totals
azmonitor-cores = Cores
azmonitor-up-time = Up Time
azmonitor-clock = Clock
azmonitor-physical-memory-k = Physical Memory (K)
azmonitor-swap-k = Swap (K)
azmonitor-total = Total
azmonitor-in-use = In Use
azmonitor-available = Available
azmonitor-free = Free
azmonitor-read = Read
azmonitor-write = Write

## Networking and users

azmonitor-network-caption = Network Utilization (scale { $scale })
azmonitor-received-rate = Received { $rate }
azmonitor-sent-rate = Sent { $rate }
azmonitor-adapter = Adapter
azmonitor-received = Received
azmonitor-sent = Sent
azmonitor-all-interfaces = All network interfaces
azmonitor-user = User
azmonitor-unknown-user = (unknown)
azmonitor-active = Active

## The status bar

azmonitor-status-processes = Processes: { $count }
azmonitor-status-processes-shown = Processes: { $shown } of { $count }
azmonitor-status-cpu = CPU Usage: { $percent }
azmonitor-status-memory = Physical Memory: { $percent }
azmonitor-sample-machine = Sample machine
azmonitor-this-computer = This computer
azmonitor-paused = Paused
azmonitor-updated-every = Updated every { $seconds } s

## Settings

azmonitor-speed-half = 0.5 s
azmonitor-speed-one = 1 s
azmonitor-speed-two = 2 s
azmonitor-update-speed = Update speed
azmonitor-history = History
azmonitor-export-minute = Export the last minute
azmonitor-export-note = The export is a CSV file in the data folder: monitor/history/<date>.csv.
azmonitor-source-sample = The readings come from the sample machine (--sample): nothing on this computer is ended.
azmonitor-source-live = The readings come from this computer (processes, CPU, memory, disks, networks).
azmonitor-export-failed = The history could not be exported: { $why }
azmonitor-exported = Exported { $key }

## Shortcuts

azmonitor-shortcut-select = Select a process
azmonitor-shortcut-screen = Move by a screen of processes
azmonitor-shortcut-first-last = The first / last process
azmonitor-shortcut-sort = Sort by a column, add a second key
azmonitor-shortcut-end = End the selected process (asks first)
azmonitor-shortcut-copy = Copy the selected rows
azmonitor-shortcut-read-now = Read the system now
azmonitor-shortcut-close-question = Close the question
