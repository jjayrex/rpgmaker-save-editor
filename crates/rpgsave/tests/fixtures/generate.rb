#!/usr/bin/env ruby
# Writes the save fixtures used by the codec tests. Run from this directory:
#   ruby generate.rb

require "fileutils"
require_relative "rgss_classes"

# ---------------------------------------------------------------- build them

def build_state(ace, binary)
  switches = Array.new(40, nil)
  [1, 2, 5, 13].each { |i| switches[i] = true }
  variables = Array.new(40, nil)
  { 1 => 7, 2 => 100, 6 => -25, 11 => 999_999 }.each { |k, v| variables[k] = v }
  self_switches = { [3, 4, gen("A", binary)] => true, [3, 9, gen("B", binary)] => true }
  actors = [nil]
  actors << Game_Actor.new(ace, 1, "Reid", "The Wanderer", 1, 12, binary)
  actors << Game_Actor.new(ace, 2, "Mirai", "Stormcaller", 2, 11, binary)
  actors << Game_Actor.new(ace, 3, "Gale", "Scout", 3, 9, binary)
  {
    system: Game_System.new(ace, binary),
    timer: Game_Timer.new,
    message: Game_Message.new,
    switches: Game_Switches.new(switches),
    variables: Game_Variables.new(variables),
    self_switches: Game_SelfSwitches.new(self_switches),
    actors: Game_Actors.new(actors),
    party: Game_Party.new(ace, binary),
    troop: Game_Troop.new,
    map: Game_Map.new(ace, binary),
    player: Game_Player.new(binary),
  }
end

# VX Ace: a header hash then a contents hash.
ace = build_state(true, false)
File.open("ace_save.rvdata2", "wb") do |f|
  header = { characters: [["Actor1", 0], ["Actor1", 1]], playtime_s: "00:25:20" }
  Marshal.dump(header, f)
  Marshal.dump(ace, f)
end

# VX: fourteen separate documents, in fixed order.
vx = build_state(false, true)
File.open("vx_save.rvdata", "wb") do |f|
  Marshal.dump([[gen("Actor1", true), 0], [gen("Actor1", true), 1]], f)
  Marshal.dump(91_235, f)
  Marshal.dump(RPG::BGM.new(gen("Town1", true), 100, 100), f)
  Marshal.dump(nil, f)
  Marshal.dump(vx[:system], f)
  Marshal.dump(vx[:message], f)
  Marshal.dump(vx[:switches], f)
  Marshal.dump(vx[:variables], f)
  Marshal.dump(vx[:self_switches], f)
  Marshal.dump(vx[:actors], f)
  Marshal.dump(vx[:party], f)
  Marshal.dump(vx[:troop], f)
  Marshal.dump(vx[:map], f)
  Marshal.dump(vx[:player], f)
end

# A grab bag of Marshal constructs the save formats do not exercise.
shared = "shared"
edge = {
  "links" => [shared, shared, shared],
  "floats" => [1.0, 0.1, -2.5, 1e300, 0.0],
  "bignums" => [2**30, -(2**30), 2**70, -(2**70)],
  "ints" => [0, 1, -1, 122, 123, -123, -124, 255, 256, -256, 2**29, -(2**29)],
  "nested" => { sym: :value, arr: [[1, [2, [3]]]], nil => false },
  "hash_default" => Hash.new(0),
  "struct" => Struct::Point.new(3, 4),
  "class" => RPG::Item,
  "regexp" => /ab+c/ix,
  "table" => Table.new(4, 4),
  "tone" => Tone.new(1.5, -2.0, 0.0, 33.0),
  "binary" => "\xff\xfe\x00\x01".b,
}
edge["hash_default"][:k] = 5
edge["cycle"] = edge
File.binwrite("edge_cases.bin", Marshal.dump(edge))

puts "wrote ace_save.rvdata2 (#{File.size('ace_save.rvdata2')} bytes)"
puts "wrote vx_save.rvdata (#{File.size('vx_save.rvdata')} bytes)"
puts "wrote edge_cases.bin (#{File.size('edge_cases.bin')} bytes)"

# ------------------------------------------------- full project directories

# A save sitting in a game folder next to its Data directory, the layout the
# editor looks for when it resolves ids to names.
def write_project(dir, ext, state, binary, header_writer)
  FileUtils.mkdir_p(File.join(dir, "Data"))
  File.write(File.join(dir, "Game.ini"),
             "[Game]\nLibrary=RGSS300.dll\nScripts=Data\\Scripts.rvdata2\nTitle=Lantern of Ys\nRTP=RPGVXAce\n")

  switches = Array.new(40, "")
  { 1 => "Met the innkeeper", 2 => "Bridge repaired", 3 => "Secret door open",
    5 => "Boat unlocked", 7 => "Heard the rumour", 13 => "Chapter 2" }
    .each { |k, v| switches[k] = gen(v, binary) }
  variables = Array.new(40, "")
  { 1 => "Quest stage", 2 => "Reputation", 6 => "Debt owed", 11 => "Score" }
    .each { |k, v| variables[k] = gen(v, binary) }

  db = {
    "System" => RPG::System.new(gen("Lantern of Ys", binary), gen("G", binary), switches, variables),
    "Items" => [nil,
      RPG::Item.new(1, gen("Potion", binary), 176, gen("Restores 200 HP.", binary), 50),
      RPG::Item.new(2, gen("Hi-Potion", binary), 176, gen("Restores 800 HP.", binary), 300),
      RPG::Item.new(3, gen("Antidote", binary), 192, gen("Cures poison.", binary), 20),
      RPG::Item.new(7, gen("Harbour Key", binary), 224, gen("Opens the dock gate.", binary), 0)],
    "Weapons" => [nil,
      RPG::Weapon.new(1, gen("Bronze Sword", binary), 96, 0, 120),
      RPG::Weapon.new(2, gen("Storm Rod", binary), 104, 0, 900)],
    "Armors" => [nil,
      RPG::Armor.new(1, gen("Leather Shield", binary), 128, 1, 90),
      RPG::Armor.new(2, gen("Travel Cloak", binary), 144, 3, 150),
      RPG::Armor.new(9, gen("Gale Charm", binary), 160, 4, 700)],
    "Skills" => [nil,
      RPG::Skill.new(1, gen("Attack", binary), 116),
      RPG::Skill.new(2, gen("Guard", binary), 128),
      RPG::Skill.new(4, gen("Spark", binary), 64),
      RPG::Skill.new(5, gen("Mend", binary), 72)],
    "States" => [nil,
      RPG::State.new(1, gen("Knockout", binary), 1),
      RPG::State.new(4, gen("Poison", binary), 16)],
    "Actors" => [nil,
      RPG::Actor.new(1, gen("Reid", binary), gen("The Wanderer", binary), 1, 1),
      RPG::Actor.new(2, gen("Mirai", binary), gen("Stormcaller", binary), 2, 1),
      RPG::Actor.new(3, gen("Gale", binary), gen("Scout", binary), 3, 1)],
    "Classes" => [nil,
      RPG::Class.new(1, gen("Wayfarer", binary), [30, 20, 30, 30], 400, 42),
      RPG::Class.new(2, gen("Stormcaller", binary), [25, 18, 30, 30], 300, 33),
      RPG::Class.new(3, gen("Scout", binary), [28, 19, 30, 30], 350, 38)],
    "MapInfos" => {
      1 => RPG::MapInfo.new(gen("World", binary)),
      3 => RPG::MapInfo.new(gen("Harbor Town", binary), 1, 2),
      12 => RPG::MapInfo.new(gen("Sunken Vault", binary), 1, 3),
    },
  }
  db.each { |name, value| File.binwrite(File.join(dir, "Data", "#{name}.#{ext}"), Marshal.dump(value)) }

  File.open(File.join(dir, "Save1.#{ext}"), "wb") { |f| header_writer.call(f, state) }
end

write_project("ace_project", "rvdata2", build_state(true, false), false, lambda { |f, s|
  Marshal.dump({ characters: [["Actor1", 0], ["Actor1", 1]], playtime_s: "00:25:20" }, f)
  Marshal.dump(s, f)
})

write_project("vx_project", "rvdata", build_state(false, true), true, lambda { |f, s|
  Marshal.dump([[gen("Actor1", true), 0], [gen("Actor1", true), 1]], f)
  Marshal.dump(91_235, f)
  Marshal.dump(RPG::BGM.new(gen("Town1", true), 100, 100), f)
  Marshal.dump(nil, f)
  [:system, :message, :switches, :variables, :self_switches,
   :actors, :party, :troop, :map, :player].each { |k| Marshal.dump(s[k], f) }
})

puts "wrote ace_project/ and vx_project/"

# Reference table for the float encoder: every line is `<bits hex>\t<marshal payload>`.
floats = [
  0.0, -0.0, 1.0, 2.0, -2.0, 0.5, -0.5, 1.5, 10.0, 100.0, -100.0, 255.0, 1234.5,
  0.1, 0.01, 0.001, 0.0001, 0.00001, -0.0001, 1e15, 1e16, 1e17, 1e18, 1e-5, 1e300,
  123456789.0, 12345678901234567.0, 3.141592653589793, 1.0 / 3.0, 6.02e23, -1.5e-7,
  99.99, 0.75, 128.0, 256.0, 1e21, 2.5e-4, 1e-3, 1e-300, 4.9e-324, 1.7976931348623157e308,
  -0.0625, 65535.0, 65536.0, 16777216.0, 1e-9, 7.0, 0.3, 1.1, 2.2250738585072014e-308,
]
File.open("floats.txt", "w") do |f|
  floats.each do |value|
    f.puts "#{[value].pack('G').unpack1('H*')}\t#{Marshal.dump(value)[4..]}"
  end
end
puts "wrote floats.txt (#{floats.size} values)"

# A save laid out the way scripted games often do it: the header carries a full
# copy of the game state so the load screen can show gold and party without
# loading the save proper, and the contents hash carries extra script data.
# Marshal writes each document independently, so the two become separate copies
# on load — edit one and the game disagrees with its own load screen.
mirrored = build_state(true, false)
File.open("mirrored_header.rvdata2", "wb") do |f|
  header = { characters: [["Actor1", 0], ["Actor1", 1]], playtime_s: "00:25:20" }
  header.merge!(mirrored)
  Marshal.dump(header, f)
  contents = mirrored.merge(script_data: { plugin: "example" })
  Marshal.dump(contents, f)
end
puts "wrote mirrored_header.rvdata2 (#{File.size('mirrored_header.rvdata2')} bytes)"
