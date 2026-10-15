# AzMeet's words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## AzMeet

azmeet-settings = Settings
azmeet-you-name = { $name } (you)
azmeet-guest = Guest
azmeet-your-screen = Your screen
azmeet-their-screen = { $name }'s screen
azmeet-tile-muted = muted
azmeet-pattern-unseen = Test pattern - not shown to anyone, not sent
azmeet-pattern = Test pattern
azmeet-camera-is-off = Camera is off
azmeet-screen-pattern = Your screen - test pattern
azmeet-tab-chat-unread = Chat ({ $count })
azmeet-tab-chat = Chat
azmeet-tab-people = People
azmeet-tab-statistics = Statistics
azmeet-safety-line = Safety code { $code }
azmeet-badge-verified = verified
azmeet-badge-speaking = speaking
azmeet-badge-deafened = deafened
azmeet-asking-to-join = Asking to join
azmeet-admit = Admit
azmeet-mark-verified = Mark verified
azmeet-no-messages = No messages yet.
azmeet-you-sending = You (sending...)
azmeet-you = You
azmeet-message-everyone = Message everyone
azmeet-send = Send
azmeet-nothing-yet = (nothing yet)
azmeet-copied = Copied
azmeet-copy-link = Copy link
azmeet-your-safety-code = Your safety code
azmeet-compare-codes = Compare it with the code each member sees for you, and theirs with yours, then mark them verified.
azmeet-sealed-failed = { $before } sealed before you joined, { $failed } that failed a check.
azmeet-members-count = Members ({ $count })
azmeet-join-call = Join call
azmeet-leave-room = Leave room
azmeet-back = Back
azmeet-mic-level = Mic level
azmeet-microphone-level = Microphone level
azmeet-mute = Mute
azmeet-unmute = Unmute
azmeet-stop-video-unseen = Stop video (not shown to anyone)
azmeet-stop-video = Stop video
azmeet-start-video = Start video
azmeet-stop-sharing = Stop sharing
azmeet-share-screen = Share screen
azmeet-undeafen = Undeafen
azmeet-deafen = Deafen
azmeet-gallery-view = Gallery view
azmeet-speaker-view = Speaker view
azmeet-drop-packet = Drop a video packet
azmeet-leave = Leave
azmeet-lobby-title = Video meetings and chat rooms
azmeet-lobby-intro = Start a meeting or a chat room, or join one with its link or its code. Everything is end-to-end encrypted.
azmeet-join-placeholder = azlin://meet/... or a code
azmeet-retry = Retry
azmeet-please-wait = Please wait...
azmeet-new-meeting = New meeting
azmeet-new-chat-room = New chat room
azmeet-minutes = minutes
azmeet-schedule = Schedule
azmeet-join-with = Join with a link or a code
azmeet-join = Join
azmeet-schedule-label = Schedule a meeting (your time, minutes)
azmeet-meeting-server = Meetings service
azmeet-loading-key = Loading this device's key...
azmeet-your-rooms = Your rooms
azmeet-none-yet = None yet.
azmeet-unread = { $count } unread
azmeet-start-meeting = Start meeting
azmeet-join-now = Join now
azmeet-ask-to-join = Ask to join
azmeet-waiting-let-in = Waiting to be let in...
azmeet-meeting-ready = Your meeting is ready
azmeet-ready-to-join = Ready to join?
azmeet-joined-with-code = You joined with the meeting's code: someone in the meeting lets you in. Compare your safety code with theirs.
azmeet-your-name = Your name
azmeet-devices = Devices
azmeet-nobody-here = No one else is here yet
azmeet-one-here = { $name } is in this meeting
azmeet-many-here = { $names } are in this meeting
azmeet-others = { $count } others
azmeet-meeting = Meeting
azmeet-microphone = Microphone
azmeet-speaker = Speaker
azmeet-camera = Camera
azmeet-category-audio-video = Audio & Video
azmeet-category-meetings = Meetings
azmeet-category-recording = Recording
azmeet-quality-auto = Automatic (up to 720p)
azmeet-quality-saver = Data saver (up to 360p)
azmeet-quality-low = Low (up to 180p)
azmeet-mirror = Mirror my video
azmeet-video-received = Video I receive
azmeet-video-quality = Video quality
azmeet-mirror-note = Mirror my video turns only your own picture, as a mirror does: the others see you the right way round.
azmeet-devices-note = The microphone and the speaker are the system's for now; the ones picked here are remembered for when AzMeet can open another.
azmeet-join-muted = Join with the microphone off
azmeet-join-camera-off = Join with the camera off
azmeet-device-safety-code = This device's safety code
azmeet-waiting-room-note = The waiting room starts with the microphone and the camera as set here; switch them there before you join.
azmeet-encryption-note = Rooms and their chats are end-to-end encrypted with this device's key: the meeting server keeps only what it cannot read. Compare safety codes with the others (a room's members list shows them) to be sure nobody stands in between.
azmeet-recordings-go-to = Recordings go to
azmeet-recording-note = Recording, a transcript made on this computer and a summary sent by mail are on the way. A recording will be kept in its meeting's folder, next to the meeting's chat and record, and travel with your Azlin storage.
azmeet-about-summary = Video meetings and chat rooms over azul.iroh, end-to-end encrypted: camera, screen sharing, the people and a chat the meeting server cannot read; each meeting's record and chat are files in the Azlin data tree.
azmeet-someone = Someone
azmeet-camera-on = camera on
azmeet-camera-off = camera off
azmeet-status-connected = connected
azmeet-status-connecting = connecting
azmeet-status-waiting = waiting for them to connect
azmeet-stats-people = People
azmeet-stats-microphones = Microphones
azmeet-stats-speakers = Speakers
azmeet-stats-video = Video
azmeet-stats-audio = Audio
azmeet-stats-network = Network
azmeet-camera-front = Front camera
azmeet-camera-back = Back camera
azmeet-camera-external = External camera
azmeet-system-default = System default
azmeet-row-knocking = waiting to be let in
azmeet-row-ended = ended
azmeet-row-members = { $count ->
    [one] { $count } member
   *[other] { $count } members
 }
azmeet-chat-room-code = Chat room { $code }
azmeet-meeting-code = Meeting { $code }
azmeet-room-member = End-to-end encrypted: only the members read it.
azmeet-room-knocking = Waiting for a member to let you in. Compare your safety code with theirs.
azmeet-room-joining = Joining...
azmeet-room-leaving = Leaving...
azmeet-room-left = You left this room.
azmeet-room-closed = This room is closed.
azmeet-room-outside = You are not in this room.
azmeet-room-key = { $count ->
    [one] Room key { $epoch }, held by the { $count } member now.
   *[other] Room key { $epoch }, held by the { $count } members now.
 }
azmeet-room-key-later = A room key is made with the first message.
azmeet-title-meeting = AzMeet · meeting { $code } · { $name }
azmeet-chat-encrypted = End-to-end encrypted
azmeet-chat-sealed-before = { $count ->
    [one] { $count } earlier message sealed before you joined
   *[other] { $count } earlier messages sealed before you joined
 }
azmeet-chat-unreadable = { $count ->
    [one] { $count } could not be read
   *[other] { $count } could not be read
 }
azmeet-identity-loading = loading...
azmeet-server-none-set = none set
azmeet-recordings-nowhere = nowhere: this run keeps no files
azmeet-recordings-folder = { $path } (each meeting's own folder)
azmeet-timed-out = timed out
azmeet-no-answer = no answer
azmeet-too-many-attempts = Too many attempts from this network; try again in a few minutes.
azmeet-server-answered = The meeting server answered { $status }. { $message }
azmeet-server-unreachable = The meeting server at { $server } is unreachable: { $why }
azmeet-link-expired = This meeting's link has expired: everyone here stays connected, nobody new can join.
azmeet-meeting-ended = This meeting has ended, or the link is wrong.
azmeet-meeting-not-encrypted = This meeting was made by an older AzMeet and is not end-to-end encrypted, so this AzMeet does not join it.
azmeet-secret-damaged = That link's secret is damaged: ask for the link again.
azmeet-room-mismatch = The meeting server sent a room that does not match this link.
azmeet-answer-unreadable = The meeting server sent an answer AzMeet cannot read.
azmeet-not-server-address = That is no meetings service address: it starts with https:// or http://.
azmeet-asking-server = Asking { $server } ...
azmeet-asking-server-from = Asking { $server } ({ $from }) ...
azmeet-server-answers = The meeting server answers.
azmeet-no-server = No meetings service is set: type its address in Settings > Meetings and press Enter, or start AzMeet with --worker <url>.
azmeet-left-room = You left the room.
azmeet-key-not-loaded = This device's key is not loaded yet: a moment, please.
azmeet-no-random = The system's random source failed: no room can be made.
azmeet-making-chat-room = Making a new chat room...
azmeet-making-meeting = Asking the meeting server for a new meeting...
azmeet-not-a-link = That is not a meeting link. Paste an azlin://meet/... link, the meeting's web address, or its code.
azmeet-looking-up = Looking up the meeting...
azmeet-schedule-bad-start = Type the start as 2026-10-09 14:00 (your time).
azmeet-schedule-bad-length = Type how long the meeting lasts, in minutes (1 to 10080).
azmeet-preview-cannot-join = This meeting is made on this device: the meetings service does not answer, so nobody can join it. Settings > Meetings has its address.
azmeet-asked-to-join = Asked to join: someone in the meeting lets you in. Compare your safety code with theirs.
azmeet-preview-no-server = A preview of the waiting room: no meeting server is set.
azmeet-preview-silent-server = A preview of the waiting room: the meetings service does not answer.
azmeet-service-down = The meetings service does not answer. Settings > Meetings has its address and Retry.
azmeet-local-meeting = This meeting is made on this device: the meetings service does not answer, so nobody can join it yet. Camera, microphone and the sound check work.
azmeet-keyring-stored = stored
azmeet-keyring-retrieved = retrieved
azmeet-keyring-deleted = deleted
azmeet-keyring-not-found = not found
azmeet-keyring-denied = denied
azmeet-keyring-unavailable = unavailable
azmeet-keyring-error = error
azmeet-keyring-file-unused = was not used: the identity file { $why }
azmeet-keyring-session = The system keyring { $why }: this device's key lives only until AzMeet quits, and its rooms do not come back after.
azmeet-no-device-key = No device key can be made ({ $why }): AzMeet cannot join a room.
azmeet-keyring-unreadable = holds a key AzMeet cannot read ({ $why })
azmeet-keyring-is = is { $state }
azmeet-keyring-not-kept = did not keep it ({ $state })
azmeet-keyring-run-only = The system keyring { $why }: this device's key lives only until AzMeet quits.
azmeet-leaving-room = Leaving the room...
azmeet-members-only-call = Only a member of the room joins its call.
azmeet-left-meeting = You left the meeting.
azmeet-server-saved = saved in the settings
azmeet-server-built-in = built in (AZMEET_DEFAULT_WORKER)
azmeet-no-network = No network link, so nobody can be reached: { $why }

## AzMeet: rooms, identity, times

azmeet-identity-keyring = kept in the system keyring
azmeet-identity-file = kept in { $path } (--identity-file)
azmeet-identity-session = for this run only: the system keyring { $why }
azmeet-server-silent-short = The meeting server does not answer.
azmeet-room-ended = This room has ended, or the link is wrong.
azmeet-message-too-long = That message is too long for the meeting server.
azmeet-link-wrong-room = This link does not open this room: ask for the link again.
azmeet-room-not-encrypted = This room was made by an older AzMeet and is not end-to-end encrypted, so this AzMeet does not join it.
azmeet-room-full = This room is full.
azmeet-another-room = The meeting server sent another room than this one.
azmeet-too-many-requests = Too many requests from this network; AzMeet tries again in a moment.
azmeet-server-refused = The meeting server answered { $status }.
azmeet-server-refused-code = The meeting server answered { $status } ({ $code }).
azmeet-span-min = { $min } min
azmeet-span-h = { $h } h
azmeet-span-h-min = { $h } h { $min } min
azmeet-span-days = { $days ->
    [one] { $days } day
   *[other] { $days } days
 }
azmeet-starts-in = starts in { $span }
azmeet-started-ago = started { $ago } ago, ends in { $left }
azmeet-ended-ago = ended { $ago } ago
azmeet-shortcuts-call = Call
azmeet-shortcut-mic = Mute or unmute the microphone
azmeet-shortcut-camera = Start or stop the camera
azmeet-room-old = This room was made by an older AzMeet and is not end-to-end encrypted, so this AzMeet does not use it.

## The meetings service and the relay, on the start screen and in the settings

azmeet-lobby-no-server = No meetings service is set.
azmeet-lobby-server-silent = The meetings service does not answer.
azmeet-connection-settings = Connection settings
azmeet-server-locked = The meetings service and the relay can be changed outside a meeting or a room.

## The sound check (the waiting room, Settings > Audio & Video)

azmeet-sound-check = Sound check
azmeet-test-speakers = Test speakers
azmeet-test-speakers-playing = Playing a sound...
azmeet-test-speakers-done = Did you hear the sound? If not, check the speaker and its volume.
azmeet-test-mic = Record a test clip
azmeet-test-mic-recording = Recording... { $seconds } s
azmeet-test-mic-playing = Playing it back...
azmeet-test-mic-done = That is how the others hear you.
azmeet-test-mic-quiet = The microphone gave only silence: check that it is plugged in and not muted, and that AzMeet may use it.
azmeet-test-mic-none = No sound came from the microphone: check that it is plugged in and that AzMeet may use it.

## The iroh relay (Settings > Meetings)

azmeet-relay = iroh relay
azmeet-relay-default = Default (n0's public relays)
azmeet-relay-azlin = Azlin (connect.azlin.io)
azmeet-relay-custom = Custom
azmeet-relay-none = None (direct connections only)
azmeet-relay-off = No relay this run (--relay off, or a meetings service on this computer): the peers connect directly.
azmeet-relay-not-address = That is no relay address: it starts with https:// or http://.
azmeet-relay-note = The relay carries a connection only when two peers cannot reach each other directly. It forwards encrypted packets it cannot read. A company can run its own and enter its address under Custom.

# Direct calls (direct_call.rs)
azmeet-direct-title = Direct call
azmeet-direct-open = Direct call...
azmeet-direct-note = A direct call reaches one other computer by its address, with no meetings service: no room, no link, no history. Only while AzMeet runs.
azmeet-direct-intro = Call another AzMeet by its address, with no meetings service in between. Read the safety words to each other when it rings: the same four words on both screens mean you reached each other.
azmeet-direct-your-address = Your address
azmeet-direct-with-addresses = With direct addresses
azmeet-direct-id-only = Id only
azmeet-direct-with-addresses-note = Faster to reach, but it shows your IP addresses to whoever has it.
azmeet-direct-id-only-note = Found through the relay: it shows no IP address.
azmeet-direct-copy = Copy
azmeet-direct-call-someone = Call an address
azmeet-direct-address-placeholder = The other computer's address
azmeet-direct-call = Call
azmeet-direct-cancel = Cancel
azmeet-direct-dialing = Calling...
azmeet-direct-ringing = Ringing. Safety words: { $words }
azmeet-direct-no-answer = Nobody answered.
azmeet-direct-busy = The other computer is in a meeting.
azmeet-direct-declined = The call was declined.
azmeet-direct-cancelled = The call was cancelled.
azmeet-direct-caller-gone = The caller hung up.
azmeet-direct-no-address = Type the other computer's address first.
azmeet-direct-yourself = That is this computer's own address.
azmeet-direct-no-network = No network connection: nothing can be called.
azmeet-direct-cannot-dial = That address cannot be called.
azmeet-direct-recent = Recent direct calls
azmeet-direct-called = You called { $name } · { $when }
azmeet-direct-was-called = { $name } called you · { $when }
azmeet-direct-calls-you = { $name } calls you directly
azmeet-direct-words = Safety words: { $words }
azmeet-direct-words-note = Read them to each other: the caller sees the same four words.
azmeet-direct-accept = Accept
azmeet-direct-decline = Decline
azmeet-direct-in-call = Direct call · safety words: { $words }
azmeet-direct-ended = The direct call ended.
