import { doctor, setup } from './setup.js';
async function main() {
    const args = process.argv.slice(2);
    const command = parseCommand(args);
    const flags = new Set(args.filter((arg) => arg.startsWith('-')));
    switch (command) {
        case 'setup':
            if (flags.has('--help') || flags.has('-h')) {
                printHelp();
                break;
            }
            await setup({
                yes: flags.has('--yes') || flags.has('-y'),
                smoke: !flags.has('--no-smoke'),
                force: flags.has('--force'),
                buildFromSource: flags.has('--build-from-source'),
                releaseBaseUrl: flagValue(args, '--release-base-url'),
                releaseDir: flagValue(args, '--release-dir'),
            });
            break;
        case 'doctor':
            await doctor();
            break;
        case 'upgrade':
            await setup({
                yes: flags.has('--yes') || flags.has('-y'),
                smoke: !flags.has('--no-smoke'),
                force: true,
                buildFromSource: flags.has('--build-from-source'),
                releaseBaseUrl: flagValue(args, '--release-base-url'),
                releaseDir: flagValue(args, '--release-dir'),
            });
            break;
        case 'version':
        case '--version':
        case '-v':
            console.log('agent_orb bootstrapper 0.1.16');
            break;
        case 'help':
        case '--help':
        case '-h':
            printHelp();
            break;
        default:
            throw new Error(`Unknown command: ${command}`);
    }
}
function parseCommand(args) {
    const first = args[0];
    if (!first)
        return 'setup';
    if (first === '--help' || first === '-h')
        return 'help';
    if (first === '--version' || first === '-v')
        return 'version';
    if (first.startsWith('-'))
        return 'setup';
    return first;
}
function flagValue(args, name) {
    const equalsPrefix = `${name}=`;
    const inline = args.find((arg) => arg.startsWith(equalsPrefix));
    if (inline)
        return inline.slice(equalsPrefix.length);
    const index = args.indexOf(name);
    if (index >= 0)
        return args[index + 1];
    return undefined;
}
function printHelp() {
    console.log(`Agent Orb bootstrapper

Usage:
  agent_orb [setup] [--yes] [--no-smoke] [--force]
            [--release-dir <dir> | --release-base-url <url>]
            [--build-from-source]
  agent_orb doctor
  agent_orb upgrade [--yes] [--no-smoke]
  agent_orb version

Local development:
  ./scripts/release/smoke-npx-local.sh
  npx --yes ./packages/agent_orb setup --yes

After npm publish:
  npx @solar_orb/agent_orb
`);
}
main().catch((error) => {
    const message = error instanceof Error ? error.message : String(error);
    console.error(`agent_orb setup failed: ${message}`);
    process.exit(1);
});
