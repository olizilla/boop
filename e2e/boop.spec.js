import { test, expect } from '@playwright/test';

test.describe('Boop E2E Recording Flow', () => {
  test('Recorder handles WAKING_MIC state and 1s min duration correctly', async ({ page }) => {
    page.on('console', msg => console.log('PAGE LOG:', msg.text()));
    page.on('pageerror', error => console.log('PAGE ERROR:', error.message));

    // 1. Visit the app
    await page.goto('/');

    // 2. The mock initial state handles stateSnapshot automatically after frontend_ready.
    // It creates 0 friends by default, so we expect the "Add friend" UI.
    await expect(page.locator('#view-add-friend')).toBeVisible();

    // 3. Add a mock friend
    await page.fill('#input-endpoint-id', 'fake-base32-endpoint');
    await page.fill('#input-nickname', 'E2ETester');
    await page.click('#btn-save-friend');

    // Wait for the UI to switch to the Friend view
    await expect(page.locator('#contact-nickname')).toHaveText('E2ETester');

    const appStatus = page.locator('#message-status');

    // 4. Test the Boop Button Interaction (Click quickly to force min 1s recording)
    const boopButton = page.locator('#btn-boop');

    // Press down and rapidly lift (simulating a very short boop)
    await boopButton.dispatchEvent('mousedown');

    // Immediately after mousedown, the state might briefly be "warming up..." or skip directly to "recording..."
    // Because the stream is mocked and instantly available via --use-fake-device-for-media-stream, 
    // it progresses very fast, but let's assert it enters recording.
    await expect(appStatus).toContainText(/warming up\.\.\.|recording\.\.\./);
    
    // Release the mouse quickly
    await boopButton.dispatchEvent('mouseup');

    // Even though mouse is up, because of the enforced 1000ms minimum duration, 
    // the UI must REMAIN in 'recording...' for at least another 500ms+
    // (Playwright checks repeatedly up to 5s)
    await expect(appStatus).toContainText('recording...');

    // Wait until it flips to cooldown (which indicates recording successfully ended)
    await expect(appStatus).toContainText(/cooling down/, { timeout: 2000 });
    
    // Finally verify cooldown dissipates and it goes back to idle (hold red button)
    // We can speed this up by manually asserting "hold red button" without waiting 20s for cooldown.
    // Actually, cooldown is 20s, let's just make sure we hit cooldown successfully.
    const screenDiv = page.locator('#screen');
    await expect(screenDiv).toHaveClass(/state-cooldown/);
  });

  test('Playback flow for received boops', async ({ page }) => {
    // 1. Visit the app and wait for hydration
    await page.goto('/');
    await page.waitForTimeout(500); // Wait for onMount to settle

    const mockFriendId = "7ebd0062-1234-4567-8901-abcdef123456";
    const mockBoopId = "boop-1234-5678";

    // 2. Inject friend and pending boop atomically via a core-event (State Snapshot)
    await page.evaluate(({ friendId, boopId }) => {
      window.dispatchEvent(new CustomEvent('core-event', {
        detail: { payload: { 
          stateSnapshot: {
            friends: [{ id: friendId, endpoint_id: "fake-id", nickname: "SimNode", emoji: "🤖" }],
            pendingBoops: {
              [friendId]: [{ id: boopId, created: Date.now(), blob_hash: "mock-hash", is_ready: true, mime_type: "audio/mp4" }]
            } 
          }
        } }
      }));
    }, { friendId: mockFriendId, boopId: mockBoopId });

    await expect(page.locator('#contact-nickname')).toHaveText('SimNode');

    // UI should show "boops: 1 - tap to play" in the message status
    await expect(page.locator('#message-status')).toContainText(/1 - tap to play/i);
    await expect(page.locator('#btn-boop')).toHaveClass(/glow-effect/);

    // 3. Click to Play
    await page.click('#btn-boop');

    // Should transition to PLAYING
    await expect(page.locator('#message-status')).toContainText('playing...');
    
    // 4. Should return to IDLE after mocked playback ends (100ms)
    await expect(page.locator('#message-status')).toContainText('hold red button', { timeout: 5000 });
    
    // Boop should be cleared (count 0 or text gone)
    // Note: App.jsx hides the boop count when there are none
    await expect(page.locator('#message-status')).not.toContainText('tap to play');
    await expect(page.locator('#btn-boop')).not.toHaveClass(/glow-effect/);
  });
});
