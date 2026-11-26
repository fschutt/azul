"""Mutations of the iroh adapter (what it makes of the engine's events, its limits and its options): each is a change to the source
that a test over the real engine must notice.

(file, old text, new text, what the change does). Run by mutate.py on a machine that has libazul (AZ_LINK_PATH, and the library
path of the system) and the bindings; AZ_REQUIRE_IROH_ENGINE=1 makes a missing engine a failure, not a skipped test."""

F = 'examples/azul-review-link/src/iroh_link.rs'

# the tests that must notice
TEST = ["cargo", "test", "-p", "azul-review-link", "--lib", "--features", "link-dynamic", "iroh_link"]
ENV = {"AZ_REQUIRE_IROH_ENGINE": "1"}

M = [
 (F,"delivery: Delivery::Latest(u8::try_from(event.track).unwrap_or(u8::MAX)),","delivery: Delivery::Latest(event.track as u8),","a track over 255 wraps round"),
 (F,"IrohEventKind::Message => LinkEvent::Received { peer: event.peer, bytes: event.data.as_ref().to_vec(), delivery: Delivery::Reliable },","IrohEventKind::Message => LinkEvent::Received { peer: event.peer, bytes: event.data.as_ref().to_vec(), delivery: Delivery::Latest(0) },","a message is newest-wins"),
 (F,"IrohEventKind::PeerConnected => LinkEvent::Connected { peer: event.peer, id: event.text.as_str().to_string() },","IrohEventKind::PeerConnected => LinkEvent::Connected { peer: event.peer, id: String::new() },","a connection has no key"),
 (F,"if [\"operation not permitted\", \"permission denied\", \"local network\", \"policydenied\", \"policy denied\"].iter().any(|words| lower.contains(words)) {","if false {","a permission failure is not told"),
 (F,"[\"operation not permitted\", \"permission denied\", \"local network\", \"policydenied\", \"policy denied\"]","[\"operation not permitted\", \"permission denied\", \"local network\"]","the iOS policy denial is not a permission failure"),
 (F,"IrohEventKind::NearbyGone => Some(NearbyEvent::Gone(event.text.as_str().to_string())),","IrohEventKind::NearbyGone => None,","a vanished neighbour is not told"),
 (F,"let advertised = String::from_utf8_lossy(event.data.as_ref()).to_string();","let advertised = String::new();","the advertised text of a neighbour is dropped"),
 (F,"        IrohEventKind::NearbyAppeared => {\n            let advertised","        IrohEventKind::Ready => {\n            let advertised","an appearing neighbour is not told"),
 (F,"let addresses = peer.addresses.as_slice().iter().map(|a| a.as_str().to_string()).collect();","let addresses = Vec::new();","a listed neighbour has no addresses"),
 (F,"        for _ in 0..MAX_EVENTS_PER_POLL {\n            let Some(event) = self.endpoint.recv().into_option() else {\n                break;\n            };","        loop {\n            let Some(event) = self.endpoint.recv().into_option() else {\n                break;\n            };","a poll takes every event"),
 (F,"            .with_max_frame_bytes(u32::try_from(MAX_FRAME_BYTES).unwrap_or(u32::MAX))\n","","the endpoint takes frames of any size"),
 (F,"Delivery::Reliable => self.endpoint.send_message(peer, U8Vec::from(bytes.to_vec())),","Delivery::Reliable => self.endpoint.send_frame(peer, 0, U8Vec::from(bytes.to_vec())),","reliable frames go as newest-wins ones"),
 (F,"Delivery::Latest(track) => self.endpoint.send_frame(peer, u32::from(track), U8Vec::from(bytes.to_vec())),","Delivery::Latest(track) => self.endpoint.send_frame(peer, 0, U8Vec::from(bytes.to_vec())),","every newest-wins frame is on track 0"),
 (F,"    if relay {\n        IrohRelayMode::Default\n    } else {\n        IrohRelayMode::Disabled\n    }","    IrohRelayMode::Default","the relay is on"),
 (F,"config = config.with_secret_key(U8Vec::from(key.to_vec()));","let _ = key;","the key is not used"),
 (F,"        if ticket.is_empty() {\n            return Err(\"there is no address to dial\".to_string());\n        }\n","","an empty ticket is dialed"),
 (F,"rtt_ms: u32::try_from(stats.rtt_us / 1_000).unwrap_or(u32::MAX),","rtt_ms: u32::try_from(stats.rtt_us).unwrap_or(u32::MAX),","the round trip is in microseconds"),
 (F,"                        if kept.len() >= MAX_NEWS {\n                            kept.remove(0);\n                        }\n","","the queue of news has no cap"),
 (F,"                            kept.remove(0);","                            kept.pop();","a full queue of news drops the newest and keeps the oldest"),
 (F,".find(|peer| peer.endpoint_id.as_str() == id)",".find(|_peer| true)","the addresses of a neighbour are those of the first one listed"),
 (F,".with_local_discovery(options.local_discovery);",".with_local_discovery(true);","the local discovery cannot be turned off"),
 (F,"        if let Some(text) = &options.advertised {\n            config = config.with_advertised(text.as_str());\n        }\n","","the advertised text is not set"),
]
