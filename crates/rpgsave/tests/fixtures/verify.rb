#!/usr/bin/env ruby
# Semantic comparison of two RGSS save files, used to prove the Rust codec
# writes something a real Ruby loads back into the same object graph.
#
#   ruby verify.rb original.rvdata2 rewritten.rvdata2
#
# Both files are loaded and re-dumped by *this* Ruby, which normalises away
# version differences such as Ruby 2.0+ interning equal floats.

require_relative "rgss_classes"

def read_documents(path)
  docs = []
  File.open(path, "rb") { |f| docs << Marshal.load(f) until f.eof? }
  docs
end

a = read_documents(ARGV[0])
b = read_documents(ARGV[1])

if a.size != b.size
  puts "DIFFER: #{a.size} documents vs #{b.size}"
  exit 1
end

mismatch = a.each_with_index.find { |doc, i| Marshal.dump(doc) != Marshal.dump(b[i]) }
if mismatch
  puts "DIFFER: document #{mismatch[1]}"
  exit 1
end

puts "MATCH: #{a.size} documents load identically"
