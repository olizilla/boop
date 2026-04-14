import { createResource, createSignal } from 'solid-js';
import { invoke } from '@tauri-apps/api/core';

export default function MyTicketView() {
  const [ticket] = createResource(() => invoke('get_my_endpoint'));
  const [copyText, setCopyText] = createSignal('Copy');

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(ticket());
      setCopyText('Copied!');
      setTimeout(() => setCopyText('Copy'), 2000);
    } catch (e) {}
  };

  return (
    <div id="view-my-ticket">
      <h2>My Endpoint ID</h2>
      <div id="my-ticket-display">{ticket() || 'Loading...'}</div>
      <button onClick={handleCopy}>{copyText()}</button>
    </div>
  );
}
