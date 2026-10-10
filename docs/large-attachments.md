# Attachment receipt and large recordings

Telecodex records the Telegram input before dispatch. A failed download must notify the originating human/topic and abort that analysis request; it does not erase or automatically replay the journaled input. A caption alone is not successful attachment delivery.

Downloads stream to a private temporary file in the destination directory. Only a completed response is published atomically, without replacing an existing file. Interrupted responses remove the partial file. Download request errors remove token-bearing URLs before logging. The getFile request and each transfer have a ten-minute deadline; the local server may need to finish downloading before it returns a path.

## Cloud limit and local Bot API

The cloud Telegram Bot API caps getFile downloads at 20 MB. A larger file can reach Telegram and still fail before reaching Codex. See [Telegram getFile](https://core.telegram.org/bots/api#getfile) and [local Bot API](https://core.telegram.org/bots/api#using-a-local-bot-api-server).

The existing telegram.api_base configuration supports a local Bot API endpoint. Use the official [telegram-bot-api server](https://github.com/tdlib/telegram-bot-api), bound to loopback, with application api_id/api_hash obtained through my.telegram.org. These are distinct from the existing bot token. Protect credentials and server state locally, and keep the endpoint out of shared body-specific deployment records.

Run the server **with --local**: this is the official mode that removes the
cloud download limit, and getFile returns an absolute filesystem path. Configure
`telegram.local_file_root` explicitly to the narrow private storage directory
shared with the bridge. The server endpoint must be loopback. Do not configure
`/`, a symlinked root or a directory containing unrelated private material.
On Unix, Telecodex opens every component descriptor-relative with O_NOFOLLOW,
rejects traversal, symlinks and non-regular files, and copies only files beneath
this boundary into its existing inbox. Absolute paths without this explicit
configuration are rejected. Non-Unix local filesystem reads remain unsupported.
A container may expose the dedicated storage at the same absolute path on both
sides; arbitrary API paths never grant host filesystem authority.

Both HTTP and local copies use private temporary files and no-clobber publication.
The final byte count must match getFile.file_size when supplied. Local copies
also match the opened file size before/after copy. A ten-minute deadline covers
transfer; failure removes partial output and never presents it to Codex.

Store application credentials in owner-private files (0600) in an owner-private
directory (0700). The official server supports TELEGRAM_API_ID and
TELEGRAM_API_HASH environment variables; a private launcher can read the files
without placing values in command arguments or logs. Never paste these values
into chat or shared records. Keep server state private and bind HTTP to loopback.

Before moving an established bot, qualify a test server, stop the one existing polling consumer while idle, follow Telegram's logOut migration procedure, update only the authorized bot's api_base, and start exactly one consumer. Preserve and verify a SQLite backup, native bindings, queued/uncertain inputs and polling offset. Capture the exact server/binary versions and rollback configuration locally. Do not reset offsets or automatically retry older failed submissions. Credentials and Bot API server availability are prerequisites, not something a Rust patch invents.

Qualification must include getMe, private-topic support on the chosen server version, a real large recording received intact, and the native model receiving its path. Loopback HTTP tests establish streaming/error behavior, not live bot migration. A deployment rollback restores software/configuration, not an old input database.
