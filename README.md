# Story

For work, we have to tag commits with the story they belong to.
When writing my commits, I truly can't be asked to skip back-and-forth to my browser.
Especially as I try to make my commit smaller and smaller.

So this is where `story` comes in.
You can invoke it, it will list up stories for from your JIRA board and let you pick one.
The story ID then gets written to a file and picked up by a `prepare-commit-message` hook and added to your commit message.
That way, while I'm working on a story I don't have to keep remembering to prefix my commits.

## Commands and options

`story install` will add the git-hook and a little entry to `.gitignore` .

`story select` will grab the stories from the board on the "In Progress" columns and display them.
You can pick from different columns using `--todo` and `--done`.

`story complete` will simply delete the `.story` file.

You can set two environment variables to influence `story`:

`NO_STORY=1` will skip prefixing and story id to the commit.

`STORY=SOME-ID` will prefix the commit with `SOME-ID: ` instead of whatever is in the `.story` file.

## Configuration

You can run `story config` to see the current configuration. Should there be no configuration file, then it will prompt you to create one.
You can also run `story config --edit` and it will use yours systems default editor to open the config.

Sampe config:

```json
{
  "jira": {
    "auth": {
      "personal_access_token": "abc-def-xyz",
      "user": "someone@jira.work"
    },
    "base_url": "https://your-compnay.atlassian.net/rest/api/2/search",
    "query": {
       "assignee": "example of your users UUID"
    }
  }
}

```
