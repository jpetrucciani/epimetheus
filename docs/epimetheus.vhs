Output epimetheus.gif

Set FontSize 12
Set Width 1200
Set Height 600
Set LetterSpacing 0

Require tmux
Require curl

Hide
  Type "tmux -f /dev/null -L test new-session -- bash" Enter
  Type "tmux split-window -d -h -p 38 -- bash && \" Enter
  Type "tmux set status && \" Enter
  Type 'tmux setw pane-border-style "fg=0" && \' Enter
  Type 'tmux setw pane-active-border-style "fg=0"' Enter
  Sleep 0.5
  Ctrl+L
  Sleep 1
Show

Type "# we have a few files that we want to scrape" Sleep 500ms Enter
Type "jq . ./test/test_basic.json" Enter
Sleep 2
Type "jq . ./test/test_nested.json" Enter
Sleep 2
Type "# lets start epimetheus! (note that we only pull out numeric values)" Sleep 500ms Enter
Sleep 1
Type "./epimetheus --files test/test_basic.json,test/test_nested.json --ignore-keys value_json_3" Sleep 500ms Enter

Ctrl+B
Type o

Sleep 1
Type "# check the metrics!" Sleep 500ms Enter
Type "curl localhost:8080/metrics" Sleep 500ms Enter
Sleep 8s
