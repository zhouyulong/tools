# Scripts Directory

This directory contains executable scripts for the dida365 sync skill.

## Files

- `dida365_sync.py` - Main synchronization script that performs the core task sync logic.

## Usage

Scripts in this directory are executed by WorkBuddy when performing dida365 task synchronization. They can be called directly for testing:

```bash
python dida365_sync.py <markdown_path> <date>
```

For example:
```bash
python dida365_sync.py ~/Documents/tasks.md 2026-03-26
```

## Development

When modifying or adding scripts:

1. Ensure the script follows Python best practices
2. Add appropriate error handling and logging
3. Update this README when adding new scripts
4. Test thoroughly before deploying
