# Story

The is a tiny little tool to automate-away a nuissance.

For work, we have to tag commits with the story they belong to. These stories are tracked in a tool called FreshRelease. When writing my commits, I truly can't be asked to skip back-and-forth to my browser. Especially as I try to make my commit smaller and smaller.

So this is where `story` comes in. You can invoke it, it will list up stories for from your Freshrelease board and let you pick one. The story ID then gets written to a file and picked up by a `prepare-commit-message` hook and added to your commit message. That way, while I'm working on a story I don't have to keep remembering to prefix my commits.



## Commands and options	

`story install` will add the git-hook and a little entry to `.gitignore` .

`story select` will grab the stories from the board on the "In Progress" columns and display them. You can pick from different columns using `--inbox` and `--priority`. These names are specific to our workflow. Sorry!.

`story complete` will simply delete the `.story` file.

You can set two environment variables to influence `story`:

`NO_STORY=1` will skip prefixing and story id to the commit.

`STORY=SOME-ID` will prefix the commit with `SOME-ID : ` instead of whatever is in the `.story` file. 



## Configuration

You can run `story config` to see the current configuration. Should there be no configuration file, then it will prompt you to create one.

You can also run `story config --edit` and it will use yours systems default editor to open the config.

Sampe config:

```json
{
  "freshrelease": {
    "base_url": "<URL to your freshrelease instance>",
    "token": "<YOUR-TOKEN-HERE>",
    "teams": [
      {
        "short_code": "...",
        "in_progress": "...",
        "priority": "...",
        "inbox": "..."
      }
    ]
  }
}

```

You can be in multiple teams and retrieve their tasks.
Each team has a `short_code` that you can get from the URL.
The `...` for `in_progress`, `priority`, and `inbox` need to be replaced with `status_id` which are sadly specific to your team/board.

# Random little notes

How to get statuses that map to columns:

```bash
http https://<YOUR INSTANCE>.freshrelease.com/<YOUR TEAM>/statuses \
	"authorization:Token <XYZ>" \
	"accept:application/json"
```



The `position` property gives the order in the column. The lower the `postition`, the further up the board:

```bash
http "https://<YOUR INSTANCE>.freshrelease.com/<YOUR TEAM SHORT CODE>/issues?query_hash[0][condition]=status_id&query_hash[0][operator]=is&query_hash[0][value]=2000000629" \
	"authorization:Token <XYZ>" \
  "accept:application/json" | jq '.issues | sort_by(.position) | reverse'
```



