# RGSS class shapes, shared by the fixture generator and the verifier.
#
# The classes below mirror the layout RPG Maker VX Ace (RGSS3) and VX (RGSS2)
# actually dump: same class names, same instance variables, same custom
# `_dump` payloads. Run with `ruby generate.rb` from this directory.


# ---------------------------------------------------------------- RGSS types

class Table
  def initialize(x, y = 0, z = 0)
    @dim = y == 0 ? 1 : (z == 0 ? 2 : 3)
    @xsize, @ysize, @zsize = x, [y, 1].max, [z, 1].max
    @data = Array.new(@xsize * @ysize * @zsize, 0)
  end
  def []=(i, v); @data[i] = v; end
  def _dump(d = 0)
    [@dim, @xsize, @ysize, @zsize, @data.size].pack("VVVVV") + @data.pack("v*")
  end
  def self._load(s)
    dim, x, y, z, size = s[0, 20].unpack("VVVVV")
    t = allocate
    t.instance_variable_set(:@dim, dim)
    t.instance_variable_set(:@xsize, x)
    t.instance_variable_set(:@ysize, y)
    t.instance_variable_set(:@zsize, z)
    t.instance_variable_set(:@data, s[20, size * 2].unpack("v*"))
    t
  end
end

class Tone
  def initialize(r, g, b, gray = 0); @r, @g, @b, @gray = r, g, b, gray; end
  def _dump(d = 0); [@r, @g, @b, @gray].pack("E4"); end
  def self._load(s); new(*s.unpack("E4")); end
end

class Color
  def initialize(r, g, b, a = 255); @r, @g, @b, @a = r, g, b, a; end
  def _dump(d = 0); [@r, @g, @b, @a].pack("E4"); end
  def self._load(s); new(*s.unpack("E4")); end
end

module RPG
  class Map
    def initialize
      @width = 20; @height = 15
      @data = Table.new(20, 15, 3)
      @data[0] = 2048
      @events = {}
      @scroll_type = 0
      @autoplay_bgm = false
      @display_name = "Harbor Town"
      @note = ""
    end
  end
  class BGM
    def initialize(n, v, p); @name, @volume, @pitch = n, v, p; end
  end
end

# ------------------------------------------------------------ shared helpers

# RGSS2 runs on Ruby 1.8 where strings have no encoding, so VX fixtures force
# binary strings to reproduce the exact bytes that engine writes.
def gen(str, binary)
  binary ? str.dup.force_encoding(Encoding::BINARY) : str
end

class Game_Switches
  def initialize(data); @data = data; end
end

class Game_Variables
  def initialize(data); @data = data; end
end

class Game_SelfSwitches
  def initialize(data); @data = data; end
end

class Game_Actors
  def initialize(data); @data = data; end
end

class Game_Interpreter
  def initialize
    @depth = 0; @branch = {}; @indent = 0; @map_id = 3
    @event_id = 0; @list = nil; @index = 0; @wait_count = 0
  end
end

class Game_Screen
  def initialize
    @brightness = 255; @fadeout_duration = 0; @fadein_duration = 0
    @tone = Tone.new(-20.0, -20.0, 0.0, 40.0)
    @tone_target = Tone.new(0.0, 0.0, 0.0, 0.0)
    @tone_duration = 0
    @flash_color = Color.new(0.0, 0.0, 0.0, 0.0)
    @flash_duration = 0
    @shake_power = 0; @shake_speed = 0; @shake_duration = 0
    @shake_direction = 1; @shake = 0
    @weather_type = :none; @weather_power = 0.0; @weather_duration = 0
    @pictures = Array.new(21) { nil }
  end
end

# --------------------------------------------------------------- VX Ace save

class Game_System
  def initialize(ace, binary)
    @save_count = 3
    @version_id = 0
    if ace
      @framecount = 91_235          # 25 min 20 s at 60 fps
      @bgm_on_save = RPG::BGM.new(gen("Town1", binary), 100, 100)
      @bgs_on_save = nil
      @windowskin_name = gen("Window", binary)
    end
    @battle_bgm = nil
    @battle_end_me = nil
    @save_disabled = false
    @menu_disabled = false
    @encounter_disabled = false
    @formation_disabled = false if ace
  end
end

class Game_Timer
  def initialize; @count = 0; @working = false; end
end

class Game_Message
  def initialize
    @texts = []; @choices = []; @face_name = ""; @face_index = 0
    @background = 0; @position = 2; @choice_cancel_type = 0
  end
end

class Game_BaseItem
  def initialize(klass = nil, id = 0); @class = klass; @item_id = id; end
end

class Game_ActionResult
  def initialize; @used = false; @missed = false; @evaded = false; end
end

class Game_Actor
  def initialize(ace, id, name, nickname, class_id, level, binary)
    @actor_id = id
    @name = gen(name, binary)
    @character_name = gen("Actor1", binary)
    @character_index = id - 1
    @face_name = gen("Actor1", binary)
    @face_index = id - 1
    @class_id = class_id
    @level = level
    @hp = 210 + level * 7
    @mp = 40 + level * 3
    @skills = [1, 2, 3 + id]
    @states = []
    @state_turns = {}
    if ace
      @nickname = gen(nickname, binary)
      @exp = { class_id => level * level * level }
      @param_plus = [0, 0, 5, 0, 2, 0, 0, 0]
      @equips = [
        Game_BaseItem.new(RPG::Weapon, id),
        Game_BaseItem.new(RPG::Armor, 1),
        Game_BaseItem.new(RPG::Armor, 0),
        Game_BaseItem.new(RPG::Armor, 0),
        Game_BaseItem.new(RPG::Armor, 2),
      ]
      @tp = 0.0
      @action_input_index = 0
      @last_skill = Game_BaseItem.new
      @state_steps = {}
      @buffs = [0, 0, 0, 0, 0, 0, 0, 0]
      @result = Game_ActionResult.new
      @hidden = false
    else
      @exp = level * level * level
      @exp_list = Array.new(100) { |i| i * i * 10 }
      @weapon_id = id
      @armor1_id = 1
      @armor2_id = 0
      @armor3_id = 0
      @armor4_id = 2
      @maxhp_plus = 0
      @maxmp_plus = 0
      @atk_plus = 5
      @def_plus = 0
      @spi_plus = 2
      @agi_plus = 0
      @two_swords_style = false
      @fix_equipment = false
      @auto_battle = false
    end
  end
end

class Game_Party
  def initialize(ace, binary)
    @gold = 12_345
    @steps = 4_207
    @actors = [1, 2]
    @items = { 1 => 9, 2 => 3, 7 => 1 }
    @weapons = { 1 => 1, 2 => 1 }
    @armors = { 1 => 2, 2 => 1 }
    @menu_actor_id = 1
    @target_actor_id = 0
    if ace
      @last_item = Game_BaseItem.new
    else
      @last_item_id = 0
    end
  end
end

class Game_Troop
  def initialize
    @interpreter = Game_Interpreter.new
    @troop_id = 0
    @event_flags = {}
    @enemies = []
    @turn_count = 0
    @in_battle = false
  end
end

class Game_Character
  def init_character
    @id = 0; @x = 8; @y = 6; @real_x = 8; @real_y = 6
    @tile_id = 0; @character_name = ""; @character_index = 0
    @direction = 2; @pattern = 1; @opacity = 255; @blend_type = 0
    @move_speed = 4; @move_frequency = 6; @walk_anime = true
    @step_anime = false; @direction_fix = false; @through = false
    @transparent = false
  end
end

class Game_Player < Game_Character
  def initialize(binary)
    init_character
    @character_name = gen("Actor1", binary)
    @vehicle_type = :walk
    @followers = nil
    @transferring = false
    @new_map_id = 0
    @new_x = 0
    @new_y = 0
    @new_direction = 0
    @encounter_count = 21
  end
end

class Game_Map
  def initialize(ace, binary)
    @map_id = 3
    @display_x = 0
    @display_y = 0
    @map = RPG::Map.new
    @events = {}
    @interpreter = Game_Interpreter.new
    @screen = Game_Screen.new
    @need_refresh = false
    @vehicles = [] if ace
    @map_name = gen("Harbor Town", binary) unless ace
  end
end


# Used by the edge-case fixture.
Struct.new("Point", :x, :y) unless defined?(Struct::Point)

# ---------------------------------------------------------- database classes

module RPG
  class BaseItem
    def initialize(id, name, icon, desc = "", price = 0)
      @id, @name, @icon_index, @description, @price = id, name, icon, desc, price
      @note = ""
      @features = []
    end
  end
  class Item < BaseItem
    def initialize(*args)
      super
      @itype_id = 1; @scope = 7; @occasion = 0; @consumable = true
    end
  end
  class EquipItem < BaseItem
    def initialize(id, name, icon, etype, price)
      super(id, name, icon, "", price)
      @etype_id = etype
      @params = [0, 0, 0, 0, 0, 0, 0, 0]
    end
  end
  class Weapon < EquipItem; end
  class Armor < EquipItem; end
  class Skill < BaseItem
    def initialize(*args)
      super
      @mp_cost = 5; @scope = 1
    end
  end
  class State < BaseItem
    def initialize(*args)
      super
      @restriction = 0
    end
  end
  class Actor < BaseItem
    def initialize(id, name, nickname, class_id, level)
      super(id, name, 0)
      @nickname = nickname
      @class_id = class_id
      @initial_level = level
      @max_level = 99
      @character_name = "Actor1"
      @character_index = id - 1
      @face_name = "Actor1"
      @face_index = id - 1
      @equips = [id, 1, 0, 0, 2]
    end
  end
  class Class < BaseItem
    def initialize(id, name, exp_params, maxhp_at_1, growth)
      super(id, name, 0)
      @exp_params = exp_params
      # 8 parameters x 100 levels, the layout VX Ace uses.
      @params = Table.new(8, 100)
      100.times do |lv|
        base = [maxhp_at_1 + growth * lv, 30 + lv * 4, 10 + lv, 8 + lv,
                9 + lv, 7 + lv, 11 + lv, 6 + lv]
        8.times { |p| @params[lv * 8 + p] = base[p] }
      end
      @learnings = []
      @features = []
    end
  end
  class MapInfo
    def initialize(name, parent = 0, order = 1)
      @name, @parent_id, @order = name, parent, order
      @expanded = false; @scroll_x = 0; @scroll_y = 0
    end
  end
  class System
    class Terms
      def initialize
        @basic = ["Level", "Lv", "HP", "HP", "MP", "MP", "TP", "TP", "Experience", "EXP"]
        @params = ["Max HP", "Max MP", "Attack", "Defense", "M.Attack",
                   "M.Defense", "Agility", "Luck", "Hit", "Evasion"]
        @etypes = ["Weapon", "Shield", "Head", "Body", "Accessory"]
        @commands = []
      end
    end
    def initialize(title, currency, switches, variables)
      @game_title = title
      @currency_unit = currency
      @switches = switches
      @variables = variables
      @terms = Terms.new
      @party_members = [1, 2]
      @elements = ["", "Physical", "Fire", "Ice"]
      @version_id = 12_345
      @start_map_id = 3
      @start_x = 8
      @start_y = 6
    end
  end
end
