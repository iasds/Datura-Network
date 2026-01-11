/**
every rung equals one order magnitude more of difficulty, here we are effectively capping max
at 54 bits targets, the higher the more units will be created but the hardest it will be to reach
**/
pub const MAX_RUNG: u32 = 10;


///Minimum diff is high enough to incur some work
pub const MIN_DIFF: u32 = 256;
