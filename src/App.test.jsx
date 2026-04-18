import { render, screen, waitFor } from '@solidjs/testing-library';
import { describe, it, expect, vi } from 'vitest';
import App from './App';
import * as tauriBridge from './tauri-bridge';

describe('Boop UI Reactivity', () => {
    it('properly updates UI upon receiving boopReceived and boopReady', async () => {
        // Render the app
        render(() => <App />);

        // Wait for the UI to be in AddFriend mode (default with empty friends)
        const mockFriendId = "7ebd0062-1234-4567-8901-abcdef123456";

        // Emit state snapshot via IPC to hydrate the store with 1 friend
        tauriBridge.mockEmit({
            "stateSnapshot": {
                friends: [{
                    id: mockFriendId,
                    endpoint_id: "fake-endpoint-id",
                    nickname: "SimNode",
                    emoji: "🤖"
                }],
                pendingBoops: {}
            }
        });

        // The UI should switch to FRIEND mode and show the friend
        expect(await screen.findByText('🤖')).toBeInTheDocument();
        expect(screen.getByText('SimNode')).toBeInTheDocument();

        // Ensure nothing is glowing or showing "boops: X" initially
        expect(screen.queryByText(/tap to play/i)).not.toBeInTheDocument();

        const mockBoopId = "boop-1234-5678";

        // 1. Emit boopReceived matching the structure Rust generates
        tauriBridge.mockEmit({
            "boopReceived": {
                friend_id: mockFriendId,
                boop: {
                    id: mockBoopId,
                    created: 123456789,
                    blob_hash: "mock-hash",
                    is_ready: false,
                    mime_type: "audio/webm"
                }
            }
        });

        // UI should show "fetching boop..." since is_ready is false
        expect(await screen.findByText('fetching boop...')).toBeInTheDocument();
        
        // Ensure "tap to play" is NOT yet present
        expect(screen.queryByText(/tap to play/i)).not.toBeInTheDocument();

        // 2. Emit boopReady indicating the chunk was explicitly fetched!
        tauriBridge.mockEmit({
            "boopReady": {
                friend_id: mockFriendId,
                boop_id: mockBoopId
            }
        });

        // UI should now reactively flip from "fetching boop..." to "boops: 1 - tap to play"
        expect(await screen.findByText(/1 - tap to play/i)).toBeInTheDocument();
        
        // Assert the glow-effect was applied to the button
        const button = document.getElementById('btn-boop');
        expect(button.className).toContain('glow-effect');
    });
});
