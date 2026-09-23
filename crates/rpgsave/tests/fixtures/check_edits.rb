#!/usr/bin/env ruby
# Reads back a save the editor has changed and prints the values a test checks:
#
#   ruby check_edits.rb edited.rvdata2
#
# Loading it in a real Ruby is the point — it shows the file is not merely
# something this project can read back to itself.
#
# This lives in a file rather than inside `ruby -e` because `require_relative`
# has no path to be relative to in `-e` code on older Rubies.
# Hopefully that's enough

require_relative "rgss_classes"

def game_objects(path)
  documents = []
  File.open(path, "rb") { |file| documents << Marshal.load(file) until file.eof? }
  # VX Ace collects the game objects into a hash; XP and VX dump them one
  # after another.
  documents.flat_map { |doc| doc.is_a?(Hash) ? doc.values : [doc] }
end

objects = game_objects(ARGV[0])
party = objects.find { |o| o.is_a?(Game_Party) }
switches = objects.find { |o| o.is_a?(Game_Switches) }

abort "no Game_Party in #{ARGV[0]}" if party.nil?
abort "no Game_Switches in #{ARGV[0]}" if switches.nil?

gold = party.instance_variable_get(:@gold)
items = party.instance_variable_get(:@items)
switch_data = switches.instance_variable_get(:@data)

puts "gold=#{gold} item3=#{items[3]} switch7=#{switch_data[7]}"
