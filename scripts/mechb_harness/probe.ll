%struct.State = type { %struct.AArch64State }
%struct.AArch64State = type { %struct.ArchState, %struct.SIMD, i64, %struct.GPR, i64, %union.anon, %union.anon, %union.anon, i64, %struct.SR, i64, %struct.SleighFlagState, [8 x i8] }
%struct.ArchState = type { i32, i32, %union.anon }
%struct.SIMD = type { [32 x %union.vec128_t] }
%union.vec128_t = type { %struct.uint128v1_t }
%struct.uint128v1_t = type { [1 x i128] }
%struct.GPR = type { i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg, i64, %struct.Reg }
%struct.Reg = type { %union.anon }
%union.anon = type { i64 }
%struct.SR = type { i64, %struct.Reg, i64, %struct.Reg, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, [4 x i8] }
%struct.SleighFlagState = type { i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, [6 x i8] }

define i64 @off_X0() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 1, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X1() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 3, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X2() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 5, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X3() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 7, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X4() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 9, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X5() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 11, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X6() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 13, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X7() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 15, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X8() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 17, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X9() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 19, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X10() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 21, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X11() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 23, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X12() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 25, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X13() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 27, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X14() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 29, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X15() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 31, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X16() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 33, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X17() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 35, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X18() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 37, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X19() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 39, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X20() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 41, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X21() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 43, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X22() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 45, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X23() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 47, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X24() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 49, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X25() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 51, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X26() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 53, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X27() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 55, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X28() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 57, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X29() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 59, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_X30() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 61, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_SP() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 63, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
define i64 @off_PC() {
  %p = getelementptr inbounds %struct.State, ptr null, i32 0, i32 0, i32 3, i32 65, i32 0, i32 0
  %v = ptrtoint ptr %p to i64
  ret i64 %v
}
