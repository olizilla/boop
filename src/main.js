import { invoke } from '@tauri-apps/api/core';

const UI = {
  screen: document.getElementById('screen'),
  contactEmoji: document.getElementById('contact-emoji'),
  contactNickname: document.getElementById('contact-nickname'),
  messageStatus: document.getElementById('message-status'),
  btnBoop: document.getElementById('btn-boop'),
  btnLeft: document.getElementById('btn-left'),
  btnRight: document.getElementById('btn-right'),
  viewAddFriend: document.getElementById('view-add-friend'),
  viewMyTicket: document.getElementById('view-my-ticket'),
  myTicketDisplay: document.getElementById('my-ticket-display'),
  inputNickname: document.getElementById('input-nickname'),
  inputTicket: document.getElementById('input-ticket'),
  btnSaveFriend: document.getElementById('btn-save-friend'),
  btnCopyTicket: document.getElementById('btn-copy-ticket'),
};

const MODE_FRIEND = 'friend';
const MODE_ADD_FRIEND = 'add_friend';
const MODE_MY_TICKET = 'my_ticket';

let state = {
  friends: [],
  currentIndex: 0,
  mode: MODE_FRIEND, 
  status: 'IDLE', // IDLE, RECORDING, PLAYING, COOLDOWN
  pendingBoops: {}, // friend_id -> queue of boops
};

let audioStream = null;
let mediaRecorder = null;
let audioChunks = [];
let audioTimeout = null;
let cooldownTimeout = null;

async function fetchFriends() {
  state.friends = await invoke('get_friends');
  if (state.friends.length === 0) {
    state.mode = MODE_ADD_FRIEND;
  } else if (state.mode !== MODE_ADD_FRIEND && state.mode !== MODE_MY_TICKET) {
    state.mode = MODE_FRIEND;
  }
  updateUI();
}

async function renderMyTicket() {
  const ticket = await invoke('get_my_endpoint');
  UI.myTicketDisplay.textContent = ticket;
}

function updateUI() {
  UI.contactEmoji.parentElement.classList.toggle('hidden', state.mode !== MODE_FRIEND);
  UI.viewAddFriend.classList.toggle('hidden', state.mode !== MODE_ADD_FRIEND);
  UI.viewMyTicket.classList.toggle('hidden', state.mode !== MODE_MY_TICKET);
  
  if (state.mode === MODE_MY_TICKET) {
    renderMyTicket();
  } else if (state.mode === MODE_FRIEND && state.friends.length > 0) {
    let friend = state.friends[state.currentIndex];
    UI.contactEmoji.textContent = friend.emoji;
    UI.contactNickname.textContent = friend.nickname;
    
    // Check pending boops
    let pending = state.pendingBoops[friend.id] || [];
    if (pending.length > 0) {
      const latest = pending[0];
      if (latest.is_ready) {
        UI.messageStatus.innerHTML = `<span class="pulse">boops: ${pending.length} - tap to play</span>`;
        UI.btnBoop.classList.add('glow-effect');
      } else {
        UI.messageStatus.innerHTML = `<span class="pulse" style="color: yellow">fetching boop...</span>`;
        UI.btnBoop.classList.remove('glow-effect');
        UI.btnBoop.style.boxShadow = 'inset 0 -4px 10px rgba(0,0,0,0.3), 0 6px 0 #901620, 0 0 15px yellow';
        
        if (!latest._downloading) {
            latest._downloading = true;
            invoke('download_boop', { friendId: friend.id, hashStr: latest.blob_hash })
              .catch(e => console.error("fetch err:", e));
        }
      }
    } else {
      UI.messageStatus.innerHTML = `hold red button to record`;
      UI.btnBoop.classList.remove('glow-effect');
      UI.btnBoop.style.boxShadow = '';
    }
  }
}

// Navigation
UI.btnLeft.addEventListener('click', () => {
  if (state.status !== 'IDLE') return;
  if (state.mode === MODE_MY_TICKET) {
    state.mode = MODE_ADD_FRIEND;
  } else if (state.mode === MODE_ADD_FRIEND) {
    if (state.friends.length > 0) {
      state.mode = MODE_FRIEND;
      state.currentIndex = state.friends.length - 1;
    }
  } else {
    // FRIEND mode
    if (state.currentIndex > 0) {
      state.currentIndex--;
    } else {
      state.mode = MODE_MY_TICKET;
    }
  }
  updateUI();
});

UI.btnRight.addEventListener('click', () => {
  if (state.status !== 'IDLE') return;
  if (state.mode === MODE_MY_TICKET) {
    if (state.friends.length > 0) {
      state.mode = MODE_FRIEND;
      state.currentIndex = 0;
    } else {
      state.mode = MODE_ADD_FRIEND;
    }
  } else if (state.mode === MODE_FRIEND) {
    if (state.currentIndex < state.friends.length - 1) {
      state.currentIndex++;
    } else {
      state.mode = MODE_ADD_FRIEND;
    }
  } else if (state.mode === MODE_ADD_FRIEND) {
    state.mode = MODE_MY_TICKET;
  }
  updateUI();
});

// Big Red Button Logic
UI.btnBoop.addEventListener('mousedown', async () => handleBtnDown());
UI.btnBoop.addEventListener('mouseup', () => handleBtnUp());
UI.btnBoop.addEventListener('mouseleave', () => handleBtnUp());

// Also support touch
UI.btnBoop.addEventListener('touchstart', async (e) => { e.preventDefault(); handleBtnDown(); });
UI.btnBoop.addEventListener('touchend', (e) => { e.preventDefault(); handleBtnUp(); });

async function handleBtnDown() {
  if (state.mode !== MODE_FRIEND) return;
  
  const friendId = state.friends[state.currentIndex].id;
  const pending = state.pendingBoops[friendId] || [];
  
  if (pending.length > 0) {
    const boopToPlay = pending[0];
    if (!boopToPlay.is_ready) return; // Still downloading
    
    // Play the boop!
    state.status = 'PLAYING';
    UI.screen.classList.add('state-playing');
    try {
      const bytes = await invoke('get_audio_bytes', { friendId, boopId: boopToPlay.blob_hash });
      const blob = new Blob([new Uint8Array(bytes)], { type: 'audio/webm' });
      const url = URL.createObjectURL(blob);
      const audio = new Audio(url);
      audio.onended = async () => {
        state.status = 'IDLE';
        UI.screen.classList.remove('state-playing');
        await invoke('mark_listened', { friendId, boopId: boopToPlay.id });
        // remove from pending locally
        state.pendingBoops[friendId].shift();
        updateUI();
      };
      audio.play();
    } catch(e) {
      console.error(e);
      state.status = 'IDLE';
      UI.screen.classList.remove('state-playing');
    }
    return;
  }
  
  // Otherwise, record
  if (state.status !== 'IDLE') return;
  
  state.status = 'RECORDING';
  UI.screen.classList.add('state-recording');
  UI.messageStatus.innerHTML = '<span class="pulse">recording...</span>';
  UI.messageStatus.classList.remove('hidden');
  
  try {
    audioStream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, sampleRate: 16000 }
    });
    mediaRecorder = new MediaRecorder(audioStream);
    audioChunks = [];
    
    mediaRecorder.ondataavailable = e => {
      audioChunks.push(e.data);
    };
    
    mediaRecorder.onstop = async () => {
      const audioBlob = new Blob(audioChunks, { type: 'audio/webm' });
      const arrayBuffer = await audioBlob.arrayBuffer();
      const bytes = Array.from(new Uint8Array(arrayBuffer));
      
      try {
        await invoke('send_boop', { friendId, audioBytes: bytes });
      } catch(e) {
        console.error("Failed to send boop", e);
      }
      
      // Cleanup
      audioStream.getTracks().forEach(t => t.stop());
      state.status = 'COOLDOWN';
      UI.screen.classList.remove('state-recording');
      
      let cd = 20;
      const interval = setInterval(() => {
        cd--;
        UI.messageStatus.innerHTML = `cooling down: ${cd}s`;
        if (cd <= 0) {
          clearInterval(interval);
          state.status = 'IDLE';
          updateUI();
        }
      }, 1000);
    };
    
    mediaRecorder.start();
    // Enforce 20s max
    audioTimeout = setTimeout(() => {
      if (state.status === 'RECORDING') {
        handleBtnUp();
      }
    }, 20000);
    
  } catch(e) {
    console.error("Mic access denied", e);
    state.status = 'IDLE';
    UI.screen.classList.remove('state-recording');
    UI.messageStatus.innerHTML = '<span class="pulse" style="color: yellow">Mic Error!</span>';
    UI.messageStatus.classList.remove('hidden');
    setTimeout(() => {
        if(state.status === 'IDLE') UI.messageStatus.classList.add('hidden');
    }, 4000);
    updateUI();
  }
}

function handleBtnUp() {
  if (state.status === 'RECORDING' && mediaRecorder && mediaRecorder.state !== 'inactive') {
    clearTimeout(audioTimeout);
    mediaRecorder.stop();
  }
}

// Polling for incoming boops
setInterval(async () => {
  if (state.friends.length === 0) return;
  
  let totalPending = 0;
  for (let friend of state.friends) {
    try {
      const boops = await invoke('get_pending_boops', { friendId: friend.id });
      state.pendingBoops[friend.id] = boops;
      totalPending += boops.length;
    } catch(e) {
      console.warn("poll err:", e);
    }
  }
  
  // Notification test - Tauri v2 relies on permissions
  
  if (state.status === 'IDLE') {
    updateUI();
  }
}, 3000);

UI.btnSaveFriend.addEventListener('click', async () => {
  const nickname = UI.inputNickname.value;
  const endpointId = UI.inputTicket.value;
  if (!nickname || !endpointId) return;
  
  try {
    await invoke('add_friend', { nickname, endpointId });
    UI.inputNickname.value = '';
    UI.inputTicket.value = '';
    await fetchFriends();
    // After adding a new friend, immediately focus on them
    if (state.friends.length > 0) {
        state.currentIndex = state.friends.length - 1;
        state.mode = 'friend';
        updateUI();
    }
  } catch(e) {
    alert("Error adding friend: " + e);
  }
});

UI.btnCopyTicket.addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText(UI.myTicketDisplay.textContent);
    UI.btnCopyTicket.textContent = "Copied!";
    setTimeout(() => UI.btnCopyTicket.textContent = "Copy", 2000);
  } catch(e) {}
});

// Init
fetchFriends();
