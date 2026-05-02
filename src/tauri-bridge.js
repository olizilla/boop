// Adapters for testing or web-only dev without Tauri backend.

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';

// To mock backend state in web
let mockState = {
    friends: [],
    pending_boops: {}
};

export const invoke = async (cmd, args) => {
    if (window.__TAURI_INTERNALS__) {
        return tauriInvoke(cmd, args);
    }
    
    console.log(`[Mock Invoke] ${cmd}`, args);
    
    // Mocks for local browser development
    if (cmd === 'frontend_ready') {
        setTimeout(() => {
            window.dispatchEvent(new CustomEvent('core-event', {
                detail: { payload: { stateSnapshot: { friends: mockState.friends, pendingBoops: mockState.pending_boops } } }
            }));
        }, 100);
    } else if (cmd === 'get_my_endpoint') {
        return "mock-endpoint-id";
    } else if (cmd === 'add_friend') {
        const id = crypto.randomUUID();
        const friend = { id, endpoint_id: args.endpointId, nickname: args.nickname, emoji: "🤖" };
        mockState.friends.push(friend);
        window.dispatchEvent(new CustomEvent('core-event', {
            detail: { payload: { friendAdded: { friend } } }
        }));
        return id;
    } else if (cmd === 'send_boop') {
        return Promise.resolve();
    } else if (cmd === 'get_audio_bytes') {
        // Generate a 1-second silent mono 8kHz WAV file
        const header = new Uint8Array([
            0x52, 0x49, 0x46, 0x46, 0x24, 0x1f, 0x00, 0x00, 0x57, 0x41, 0x56, 0x45, 0x66, 0x6d, 0x74, 0x20,
            0x10, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x40, 0x1f, 0x00, 0x00, 0x40, 0x1f, 0x00, 0x00,
            0x01, 0x00, 0x08, 0x00, 0x64, 0x61, 0x74, 0x61, 0x00, 0x1f, 0x00, 0x00
        ]);
        const data = new Uint8Array(8000).fill(0x80); // Silence for 8-bit PCM is 128 (0x80)
        const wav = new Uint8Array(header.length + data.length);
        wav.set(header);
        wav.set(data, header.length);
        return wav;
    } else if (cmd === 'mark_listened') {
        return Promise.resolve();
    } else if (cmd === 'play_boop') {
        return new Promise(resolve => setTimeout(resolve, 500));
    }
    return null;
};

export const listen = async (event, callback) => {
    if (window.__TAURI_INTERNALS__) {
        return tauriListen(event, callback);
    }
    
    console.log(`[Mock Listen] Subscribed to ${event}`);
    const handler = (e) => {
        callback(e.detail);
    };
    
    window.addEventListener(event, handler);
    return () => {
        window.removeEventListener(event, handler);
    };
};

export const mockEmit = (payload) => {
    window.dispatchEvent(new CustomEvent('core-event', {
        detail: { payload }
    }));
};
// fix ui glitch on linux on arm. see: https://github.com/olizilla/boop/issues/1
export const showWindow = async () => {
    if (window.__TAURI_INTERNALS__) {
        return getCurrentWebviewWindow().show();
    }
    console.log("[Mock] Window shown");
};
