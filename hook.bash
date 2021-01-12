#!/bin/sh

# only run if we are interactive
if [[ "$2" != "" ]]; then
  exit 0
fi

# allow disabling of story
if [[ "$NO_STORY" == "1" ]]; then
  exit 0
fi


story_file="$(git rev-parse --show-toplevel)/.story"

if [[ -f "$story_file" ]]; then
  source $story_file
  sed -i '' -e  "1s/.*/$story_id : /" $1
fi
