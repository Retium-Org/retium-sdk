// Prints the RCP-1 metadata as the byte list assembly/index.ts embeds.
// Edit the record below, run `node metadata.mjs`, paste the output over the list.
const metadata = {
  rcp_type: 'counter',
  traits: ['demo'],
  capabilities: ['execute'],
  lockable_fields: [],
  evolves_from: '',
  weight: 5,
  version: 'v1.0.0',
};
console.log([...Buffer.from(`RCP_METADATA:${JSON.stringify(metadata)}\0`)].join(', '));
