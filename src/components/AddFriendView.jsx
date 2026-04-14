import { createSignal } from 'solid-js';
import { invoke } from '@tauri-apps/api/core';

export default function AddFriendView(props) {
  const [nickname, setNickname] = createSignal('');
  const [ticket, setTicket] = createSignal('');

  const handleSave = async () => {
    const nick = nickname();
    const tkt = ticket();
    if (!nick || !tkt) return;

    try {
      await invoke('add_friend', { nickname: nick, endpointId: tkt });
      setNickname('');
      setTicket('');
      props.onSaved();
    } catch (e) {
      alert("Error adding friend: " + e);
    }
  };

  return (
    <div id="view-add-friend">
      <h2>Add Friend</h2>
      <input 
        type="text" 
        placeholder="Nickname" 
        value={nickname()} 
        onInput={(e) => setNickname(e.currentTarget.value)} 
      />
      <textarea 
        placeholder="Paste Endpoint ID..." 
        value={ticket()} 
        onInput={(e) => setTicket(e.currentTarget.value)}
      ></textarea>
      <button onClick={handleSave}>Save</button>
    </div>
  );
}
