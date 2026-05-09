import { createSignal } from 'solid-js';
import { invoke } from '../tauri-bridge';

export default function JoinFriendView(props) {
	const [nickname, setNickname] = createSignal('');
	const [ticket, setTicket] = createSignal('');
	const [isLoading, setIsLoading] = createSignal(false);

	const handleJoin = async () => {
		if (!nickname() || !ticket()) return;
		setIsLoading(true);
		try {
			await invoke('accept_invite', { ticketStr: ticket().trim(), nickname: nickname() });
			setNickname('');
			setTicket('');
			props.onJoined?.();
		} catch (e) {
			alert("Error joining friend: " + e);
		} finally {
			setIsLoading(false);
		}
	};

	return (
		<div id="view-join-friend">
			<h2>Join Friend</h2>
			<div class="join-form">
				<p>Paste an invite ticket from a friend and give them a nickname.</p>
				<input 
					id="input-join-nickname"
					type="text" 
					placeholder="Friend's Nickname" 
					value={nickname()} 
					onInput={(e) => setNickname(e.currentTarget.value)} 
					disabled={isLoading()}
				/>
				<textarea 
					id="input-join-ticket"
					placeholder="Paste Invite Ticket..." 
					value={ticket()} 
					onInput={(e) => setTicket(e.currentTarget.value)}
					disabled={isLoading()}
				></textarea>
				<button 
					id="btn-join-friend" 
					onClick={handleJoin}
					disabled={!nickname() || !ticket() || isLoading()}
				>
					{isLoading() ? 'Joining...' : 'Join Friend'}
				</button>
			</div>
		</div>
	);
}
