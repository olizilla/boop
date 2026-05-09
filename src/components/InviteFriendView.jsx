import { createSignal, Show } from 'solid-js';
import { invoke } from '../tauri-bridge';

export default function InviteFriendView(props) {
	const [nickname, setNickname] = createSignal('');
	const [ticket, setTicket] = createSignal('');
	const [copyText, setCopyText] = createSignal('Copy Invite');
	const [isLoading, setIsLoading] = createSignal(false);

	const handleGenerate = async () => {
		if (!nickname()) return;
		setIsLoading(true);
		try {
			const res = await invoke('generate_invite', { petName: nickname() });
			setTicket(res);
		} catch (e) {
			alert("Error generating invite: " + e);
		} finally {
			setIsLoading(false);
		}
	};

	const handleCopy = async () => {
		try {
			await navigator.clipboard.writeText(ticket());
			setCopyText('Copied!');
			setTimeout(() => setCopyText('Copy Invite'), 2000);
		} catch (e) {}
	};

	const handleReset = () => {
		setNickname('');
		setTicket('');
	};

	return (
		<div id="view-invite-friend">
			<h2>Invite Friend</h2>
			<Show when={!ticket()} fallback={
				<div class="invite-success">
					<p>Invite for <b>{nickname()}</b> ready:</p>
					<div id="invite-ticket-display">{ticket()}</div>
					<div class="invite-actions">
						<button onClick={handleCopy}>{copyText()}</button>
						<button class="secondary" onClick={handleReset}>New Invite</button>
					</div>
				</div>
			}>
				<div class="invite-form">
					<p>Set a nickname for your friend. Only you will see this name.</p>
					<input 
						id="input-invite-nickname"
						type="text" 
						placeholder="Friend's Nickname" 
						value={nickname()} 
						onInput={(e) => setNickname(e.currentTarget.value)} 
						disabled={isLoading()}
					/>
					<button 
						id="btn-generate-invite" 
						onClick={handleGenerate}
						disabled={!nickname() || isLoading()}
					>
						{isLoading() ? 'Generating...' : 'Generate Invite Ticket'}
					</button>
				</div>
			</Show>
		</div>
	);
}
