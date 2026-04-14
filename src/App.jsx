import { createSignal, createEffect, onMount, onCleanup, Show, Match, Switch } from 'solid-js';
import { createStore } from 'solid-js/store';
import { invoke } from '@tauri-apps/api/core';
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

  const currentFriend = () => friends()[currentIndex()];

  const fetchFriends = async () => {
    const list = await invoke('get_friends');
    setFriends(list);
    if (list.length === 0) {
      setMode(MODE_ADD_FRIEND);
    } else if (mode() === MODE_FRIEND && currentIndex() >= list.length) {
      setCurrentIndex(0);
    }
  };

  // Polling & Focus listeners
  onMount(() => {
    fetchFriends();
    
    const handleFocus = () => setIsFocused(true);
    const handleBlur = () => setIsFocused(false);
    window.addEventListener('focus', handleFocus);
    window.addEventListener('blur', handleBlur);

    const interval = setInterval(async () => {
      if (friends().length === 0) {
        await fetchFriends();
        if (friends().length === 0) return;
      }

      for (const friend of friends()) {
        try {
          const boops = await invoke('get_pending_boops', { friendId: friend.id });
          // Preserve our local _downloading flags if they exist
          const existing = pendingBoops[friend.id] || [];
          const updated = boops.map(nb => {
              const matching = existing.find(eb => eb.id === nb.id);
              return matching ? { ...nb, _downloading: matching._downloading } : nb;
          });
          setPendingBoops(friend.id, updated);
        } catch (e) {
          console.warn("poll err:", e);
        }
      }
    }, 3000);

    // Auto-downloader effect
    createEffect(() => {
        Object.keys(pendingBoops).forEach(friendId => {
            const list = pendingBoops[friendId];
            if (list && list.length > 0) {
                const latest = list[0];
                if (!latest.is_ready && !latest._downloading) {
                    setPendingBoops(friendId, 0, '_downloading', true);
                    invoke('download_boop', { friendId, hashStr: latest.blob_hash })
                        .catch(e => {
                            console.error("fetch err:", e);
                            setPendingBoops(friendId, 0, '_downloading', false);
                        });
                }
            }
        });
    });

    onCleanup(() => {
      clearInterval(interval);
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
        // Trigger download if not already
        if (!boopToPlay._downloading) {
            boopToPlay._downloading = true; // Local flag
            invoke('download_boop', { friendId: friend.id, hashStr: boopToPlay.blob_hash })
              .catch(e => console.error("fetch err:", e));
        }
        return;
      }
      
      // Play
      setStatus('PLAYING');
      try {
        const bytes = await invoke('get_audio_bytes', { friendId: friend.id, boopId: boopToPlay.blob_hash });
        const blob = new Blob([new Uint8Array(bytes)], { type: boopToPlay.mime_type });
        const url = URL.createObjectURL(blob);
        const audio = new Audio(url);
        audio.onended = async () => {
          setStatus('IDLE');
          await invoke('mark_listened', { friendId: friend.id, boopId: boopToPlay.id });
          const updated = [...pendingBoops[friend.id]];
          updated.shift();
          setPendingBoops(friend.id, updated);
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
    setStatus('RECORDING');
    try {
      audioStream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, sampleRate: 16000 }
      });
      const options = { mimeType: 'audio/webm;codecs=opus' };
      mediaRecorder = MediaRecorder.isTypeSupported(options.mimeType) 
        ? new MediaRecorder(audioStream, options) 
        : new MediaRecorder(audioStream);

      audioChunks = [];
      mediaRecorder.ondataavailable = e => audioChunks.push(e.data);
      mediaRecorder.onstop = async () => {
        const audioBlob = new Blob(audioChunks);
        const arrayBuffer = await audioBlob.arrayBuffer();
        const bytes = Array.from(new Uint8Array(arrayBuffer));
        
        try {
          await invoke('send_boop', { 
            friendId: currentFriend().id, 
            audioBytes: bytes, 
            mimeType: mediaRecorder.mimeType 
          });
        } catch(e) { console.error("Send failed", e); }
        
        audioStream.getTracks().forEach(t => t.stop());
        startCooldown();
      };
      mediaRecorder.start();
      audioTimeout = setTimeout(() => handleBoopUp(), 20000);
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
    if (status() === 'RECORDING' && mediaRecorder && mediaRecorder.state !== 'inactive') {
      clearTimeout(audioTimeout);
      mediaRecorder.stop();
    }
  };

  return (
    <div id="arcade-cabinet" classList={{ 'faded': !isFocused() }}>
      <div id="screen" classList={{
        'state-idle': status() === 'IDLE',
        'state-playing': status() === 'PLAYING',
        'state-recording': status() === 'RECORDING',
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
              <AddFriendView onSaved={() => { fetchFriends(); setMode(MODE_FRIEND); setCurrentIndex(friends().length - 1); }} />
            </Match>
            <Match when={mode() === MODE_FRIEND && currentFriend()}>
              <div class="contact-info">
                <span id="contact-emoji">{currentFriend().emoji}</span>
                <span id="contact-nickname">{currentFriend().nickname}</span>
              </div>
              
              <div id="message-status">
                <Switch fallback={<span>hold red button to record</span>}>
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
                        <Show when={pendingBoops[currentFriend().id][0].is_ready} 
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
