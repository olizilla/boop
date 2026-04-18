import { createSignal, onMount, onCleanup, Show, Match, Switch } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { invoke, listen, showWindow } from './tauri-bridge';
import AddFriendView from './components/AddFriendView';
import MyTicketView from './components/MyTicketView';

const MODE_FRIEND = 'friend';
const MODE_ADD_FRIEND = 'add_friend';
const MODE_MY_TICKET = 'my_ticket';

export default function App() {
	const [friends, setFriends] = createSignal([]);
	const [currentIndex, setCurrentIndex] = createSignal(0);
	const [mode, setMode] = createSignal(MODE_FRIEND);
	const [status, setStatus] = createSignal('IDLE'); // IDLE, RECORDING, PLAYING, COOLDOWN
	const [pendingBoops, setPendingBoops] = createStore({}); // friend_id -> Boop[]
	const [cooldown, setCooldown] = createSignal(0);
	const [isFocused, setIsFocused] = createSignal(true);

	let audioStream = null;
	let mediaRecorder = null;
	let audioChunks = [];
	let audioTimeout = null;
	let gettingStream = null;
	let recordStartTime = 0;
	let isBoopPressed = false;

	const warmUpMic = async () => {
		if (audioStream) return audioStream;
		if (gettingStream) return gettingStream;
		
		gettingStream = navigator.mediaDevices.getUserMedia({
			audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, sampleRate: 16000 }
		}).then(stream => {
			audioStream = stream;
			gettingStream = null;
			return stream;
		}).catch(e => {
			gettingStream = null;
			throw e;
		});
		return gettingStream;
	};

	const currentFriend = () => friends()[currentIndex()];

	onMount(async () => {
		const handleFocus = () => setIsFocused(true);
		const handleBlur = () => setIsFocused(false);
		window.addEventListener('focus', handleFocus);
		window.addEventListener('blur', handleBlur);

		// Pre-warm the mic so first record is instant
		warmUpMic().catch(() => {});

		const unlisten = await listen('core-event', (ev) => {
			const rawPayload = ev.payload;
			// Tauri serde yields either `{ type: '...', ... }` if tagged properly, 
			// or `{ StateSnapshot: { ... } }` depending on derive macro.
			// Let's handle Rust enum tagging explicitly
			const typeKey = typeof rawPayload === 'object' && rawPayload !== null ? Object.keys(rawPayload)[0] : null;
			if (!typeKey) return;
			
			const payload = rawPayload[typeKey];

			switch (typeKey) {
				case 'stateSnapshot':
					setFriends(payload.friends);
					Object.entries(payload.pendingBoops).forEach(([k, v]) => setPendingBoops(k, v));
					if (payload.friends.length === 0) {
						setMode(MODE_ADD_FRIEND);
					} else {
						setMode(MODE_FRIEND);
						setCurrentIndex(0);
					}
					break;
				case 'friendAdded':
					const nextIndex = friends().length;
					setFriends(f => [...f, payload.friend]);
					setCurrentIndex(nextIndex);
					setMode(MODE_FRIEND);
					break;
				case 'boopReceived':
					console.log("[CoreEvent] boopReceived", payload);
					setPendingBoops(produce(draft => {
						const fId = payload.friend_id;
						if (!draft[fId]) draft[fId] = [];
						draft[fId].push(payload.boop);
					}));
					break;
				case 'boopReady':
					console.log("[CoreEvent] boopReady", payload);
					setPendingBoops(produce(draft => {
						const arr = draft[payload.friend_id];
						if (arr) {
							const idx = arr.findIndex(b => b.id === payload.boop_id);
							if (idx !== -1) arr[idx].is_ready = true;
						}
					}));
					break;
				default:
					break;
			}
		});

		await invoke('frontend_ready');
		// fix ui glitch on linux on arm. see: https://github.com/olizilla/boop/issues/1
		await showWindow();

		onCleanup(() => {
			if (typeof unlisten === 'function') unlisten();
			window.removeEventListener('focus', handleFocus);
			window.removeEventListener('blur', handleBlur);
		});
	});

	const handleLeft = () => {
		if (status() !== 'IDLE') return;
		if (mode() === MODE_MY_TICKET) {
			setMode(MODE_ADD_FRIEND);
		} else if (mode() === MODE_ADD_FRIEND) {
			if (friends().length > 0) {
				setMode(MODE_FRIEND);
				setCurrentIndex(friends().length - 1);
			}
		} else {
			if (currentIndex() > 0) {
				setCurrentIndex(currentIndex() - 1);
			} else {
				setMode(MODE_MY_TICKET);
			}
		}
	};

	const handleRight = () => {
		if (status() !== 'IDLE') return;
		if (mode() === MODE_MY_TICKET) {
			if (friends().length > 0) {
				setMode(MODE_FRIEND);
				setCurrentIndex(0);
			} else {
				setMode(MODE_ADD_FRIEND);
			}
		} else if (mode() === MODE_FRIEND) {
			if (currentIndex() < friends().length - 1) {
				setCurrentIndex(currentIndex() + 1);
			} else {
				setMode(MODE_ADD_FRIEND);
			}
		} else if (mode() === MODE_ADD_FRIEND) {
			setMode(MODE_MY_TICKET);
		}
	};

	const handleBoopDown = async () => {
		if (mode() !== MODE_FRIEND || status() !== 'IDLE') return;
		
		const friend = currentFriend();
		const pending = pendingBoops[friend.id] || [];
		
		if (pending.length > 0) {
			const boopToPlay = pending[0];
			if (!boopToPlay.is_ready) {
				return; // Download handled by core automatically
			}
			
			// Play
			setStatus('PLAYING');
			try {
				const bytes = await invoke('get_audio_bytes', { friendId: friend.id, boopId: boopToPlay.blob_hash });
				const sanitizedType = boopToPlay.mime_type.split(';')[0]; // see: https://github.com/olizilla/boop/issues/4
				const blob = new Blob([new Uint8Array(bytes)], { type: sanitizedType });
				const url = URL.createObjectURL(blob);
				const audio = new Audio(url);
				audio.onended = async () => {
					setStatus('IDLE');
					await invoke('mark_listened', { friendId: friend.id, boopId: boopToPlay.id });
					setPendingBoops(produce(draft => {
						if (draft[friend.id]) draft[friend.id].shift();
					}));
				};
				audio.play();
			} catch (e) {
				console.error(e);
				setStatus('IDLE');
			}
		} else {
			// Record
			startRecording();
		}
	};

	const startRecording = async () => {
		isBoopPressed = true;
		setStatus('WAKING_MIC');
		try {
			await warmUpMic();
			
			
			const supportedTypes = [
				'audio/webm;codecs=opus',
				'audio/mp4'
			];
			const selectedType = supportedTypes.find(type => MediaRecorder.isTypeSupported(type)) || '';
			
			mediaRecorder = selectedType 
				? new MediaRecorder(audioStream, { mimeType: selectedType }) 
				: new MediaRecorder(audioStream);

			audioChunks = [];
			mediaRecorder.ondataavailable = e => audioChunks.push(e.data);
			
			mediaRecorder.onstart = () => {
				if (!isBoopPressed) {
					// User released the button very quickly while waking mic, enforce minimum 1s record
					setStatus('RECORDING');
					setTimeout(() => {
						if (mediaRecorder && mediaRecorder.state !== 'inactive') mediaRecorder.stop();
					}, 1000);
				} else {
					setStatus('RECORDING');
					recordStartTime = Date.now();
				}
			};

			mediaRecorder.onstop = async () => {
				// Ensure the blob is typed with the actual recorder mimeType (without codecs for max compatibility)
				const sanitizedType = mediaRecorder.mimeType.split(';')[0];
				const audioBlob = new Blob(audioChunks, { type: sanitizedType });
				const arrayBuffer = await audioBlob.arrayBuffer();
				const bytes = Array.from(new Uint8Array(arrayBuffer));
				
				try {
					await invoke('send_boop', { 
						friendId: currentFriend().id, 
						audioBytes: bytes, 
						mimeType: mediaRecorder.mimeType 
					});
				} catch(e) { console.error("Send failed", e); }
				
				// We no longer stop the tracks here so the stream stays warm!
				startCooldown();
			};
			mediaRecorder.start();
			audioTimeout = setTimeout(() => {
				isBoopPressed = false;
				handleBoopUp();
			}, 20000);
		} catch (e) {
			console.error(e);
			setStatus('IDLE');
		}
	};

	const startCooldown = () => {
		setStatus('COOLDOWN');
		setCooldown(20);
		const interval = setInterval(() => {
			setCooldown(c => c - 1);
			if (cooldown() <= 0) {
				clearInterval(interval);
				setStatus('IDLE');
			}
		}, 1000);
	};

	const handleBoopUp = () => {
		isBoopPressed = false;
		if ((status() === 'RECORDING' || status() === 'WAKING_MIC') && mediaRecorder && mediaRecorder.state !== 'inactive') {
			clearTimeout(audioTimeout);
			if (status() === 'RECORDING') {
				const elapsed = Date.now() - recordStartTime;
				if (elapsed < 1000) {
					// Guarantee minimum 1s recording
					setTimeout(() => {
						if (mediaRecorder && mediaRecorder.state !== 'inactive') mediaRecorder.stop();
					}, 1000 - elapsed);
				} else {
					mediaRecorder.stop();
				}
			}
			// If WAKING_MIC, we let the onstart callback handle the 1s wrap-up
		}
	};

	return (
		<div id="arcade-cabinet" classList={{ 'faded': !isFocused() }}>
			<div id="screen" classList={{
				'state-idle': status() === 'IDLE',
				'state-playing': status() === 'PLAYING',
				'state-recording': status() === 'RECORDING' || status() === 'WAKING_MIC',
				'state-cooldown': status() === 'COOLDOWN'
			}}>
				<div id="screen-glare"></div>
				<div id="header">
					<div id="status-indicator" class="online"></div>
					<h1>BOOP</h1>
				</div>

				<div id="content-area">
					<Switch>
						<Match when={mode() === MODE_MY_TICKET}>
							<MyTicketView />
						</Match>
						<Match when={mode() === MODE_ADD_FRIEND}>
							<AddFriendView onSaved={() => {}} />
						</Match>
						<Match when={mode() === MODE_FRIEND && currentFriend()}>
							<div class="contact-info">
								<span id="contact-emoji">{currentFriend().emoji}</span>
								<span id="contact-nickname">{currentFriend().nickname}</span>
							</div>
							
							<div id="message-status">
								<Switch fallback={<span>hold red button to record</span>}>
									<Match when={status() === 'WAKING_MIC'}>
										<span class="pulse">warming up...</span>
									</Match>
									<Match when={status() === 'RECORDING'}>
										<span class="pulse">recording...</span>
									</Match>
									<Match when={status() === 'COOLDOWN'}>
										<span>cooling down: {cooldown()}s</span>
									</Match>
									<Match when={status() === 'PLAYING'}>
										<span class="pulse">playing...</span>
									</Match>
									<Match when={(pendingBoops[currentFriend().id] || []).length > 0}>
										<div class="pulse">
												<Show when={pendingBoops[currentFriend().id]?.[0]?.is_ready} 
															fallback={<span style="color: yellow">fetching boop...</span>}>
														boops: {pendingBoops[currentFriend().id].length} - tap to play
												</Show>
										</div>
									</Match>
								</Switch>
							</div>
						</Match>
					</Switch>
				</div>
			</div>

			<div id="controls">
				<button id="btn-left" class="arcade-btn small-btn" onClick={handleLeft}>◀</button>
				<div class="big-btn-container">
					<button 
						id="btn-boop" 
						class="arcade-btn big-red-btn" 
						classList={{ 'glow-effect': (pendingBoops[currentFriend()?.id] || []).some(b => b.is_ready) }}
						onMouseDown={handleBoopDown}
						onMouseUp={handleBoopUp}
						onMouseLeave={handleBoopUp}
						onTouchStart={(e) => { e.preventDefault(); handleBoopDown(); }}
						onTouchEnd={(e) => { e.preventDefault(); handleBoopUp(); }}
					></button>
				</div>
				<button id="btn-right" class="arcade-btn small-btn" onClick={handleRight}>▶</button>
			</div>
		</div>
	);
}
