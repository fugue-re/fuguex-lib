@0xbb592cfa8049750f;		# Capnp Unique ID by `capnp id`

struct TBBBlocks {
	basicBlocks @0	: List(TBBBlock);
}

struct TBBBlock {
	address @0 	: UInt64;
	n 		@1 	: UInt32;
	thumbMode	@2 	: Bool;
	isync 		@3	: UInt32; 
}
