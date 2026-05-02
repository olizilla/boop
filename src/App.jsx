import { createSignal, onMount, onCleanup, Show, Match, Switch } from 'solid-js';
import { encodeWAV } from './audio';
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
	let cooldownInterval = null;
	let audioContext = null;

	const warmUpMic = async () => {
		if (audioStream) return audioStream;
		if (gettingStream) return gettingStream;
		
		gettingStream = navigator.mediaDevices.getUserMedia({
			audio: { 
				channelCount: 1, 
				echoCancellation: true, 
				noiseSuppression: true, 
				autoGainControl: true, 
				sampleRate: 16000 
			}
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
					if (status() === 'COOLDOWN') {
						clearInterval(cooldownInterval);
						setStatus('IDLE');
					}
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
					if (status() === 'COOLDOWN') {
						clearInterval(cooldownInterval);
						setStatus('IDLE');
					}
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
			
			// Play via backend
			setStatus('PLAYING');
			try {
				await invoke('play_boop', { friendId: friend.id, boopId: boopToPlay.id });
				
				// Backend returns only when playback finishes
				setPendingBoops(produce(draft => {
					if (draft[friend.id]) draft[friend.id].shift();
				}));
				setStatus('IDLE');
			} catch (e) {
				console.error("Playback failed", e);
				setStatus('ERROR');
				
				setTimeout(() => {
					if (status() === 'ERROR') setStatus('IDLE');
				}, 2000);
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
				'audio/webm',
				'audio/mp4',
				'audio/ogg;codecs=opus',
				'audio/ogg'
			];

			// Debug log for PI troubleshooting
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
				const sanitizedType = mediaRecorder.mimeType.split(';')[0];
				const originalBlob = new Blob(audioChunks, { type: sanitizedType });
				const originalBuffer = await originalBlob.arrayBuffer();
				
				console.log(`[Recording] Original size: ${originalBuffer.byteLength} bytes (${sanitizedType})`);

				// Transcode to WAV (Mono, 16-bit PCM)
				let finalBytes;
				let finalType = 'audio/wav';
				try {
					if (!audioContext) audioContext = new (window.AudioContext || window.webkitAudioContext)();
					const audioBuffer = await audioContext.decodeAudioData(originalBuffer);
					finalBytes = encodeWAV(audioBuffer);
					console.log(`[Recording] Transcoded to WAV. Size: ${finalBytes.length} bytes. Mono @ ${audioBuffer.sampleRate}Hz`);
				} catch (e) {
					console.error("[Recording] Transcoding failed, falling back to original blob", e);
					finalBytes = new Uint8Array(originalBuffer);
					finalType = sanitizedType;
				}

				if (finalBytes.length < 100) {
					console.warn("[Recording] Data too small, ignoring.");
					setStatus('IDLE');
					return;
				}

				try {
					await invoke('send_boop', { 
						friendId: currentFriend().id, 
						audioBytes: Array.from(finalBytes), 
						mimeType: finalType 
					});
				} catch(e) { console.error("Send failed", e); }
				
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
		clearInterval(cooldownInterval);
		cooldownInterval = setInterval(() => {
			setCooldown(c => c - 1);
			if (cooldown() <= 0) {
				clearInterval(cooldownInterval);
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
					// Add a small buffer delay (500ms) to ensure the last bit of audio is captured
					setTimeout(() => {
						if (mediaRecorder && mediaRecorder.state !== 'inactive') mediaRecorder.stop();
					}, 300);
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
									<Match when={status() === 'ERROR'}>
										<span style="color: #ff4444">playback failed!</span>
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
