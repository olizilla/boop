// Adapters for testing or web-only dev without Tauri backend.

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';

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
