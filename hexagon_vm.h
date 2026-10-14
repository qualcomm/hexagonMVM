/*
 * Copyright (c) 2024 Taylor Simpson <ltaylorsimpson@gmail.com>
 * Copyright (c) 2024 Qualcomm Innovation Center, Inc. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 */

#ifndef _HEXAGON_VM_H
#define _HEXAGON_VM_H

// Virtual instructions
#define vmversion trap1(#0)
#define vmrte     trap1(#1)
#define vmsetvec  trap1(#2)
#define vmsetie   trap1(#3)
#define vmgetie   trap1(#4)
#define vmintop   trap1(#5)
#define vmclrmap  trap1(#10)
/* Translation types for vmnewmap's r1 argument */
#define HVM_TRANS_TYPE_LINEAR 0
#define HVM_TRANS_TYPE_TABLE  1
#define HVM_TRANS_TYPE_OFFSET 2

#define vmnewmap  trap1(#11)
#define vmcache   trap1(#13)
#define vmgettime trap1(#14)
#define vmsettime trap1(#15)
#define vmwait    trap1(#16)
#define vmyield   trap1(#17)
#define vmstart   trap1(#18)
#define vmstop    trap1(#19)
#define vmvpid    trap1(#20)
#define vmsetregs trap1(#21)
#define vmgetregs trap1(#22)

// PTE page size indicators
#define HVM_PTE_PGSIZE_4K      0
#define HVM_PTE_PGSIZE_16K     1
#define HVM_PTE_PGSIZE_64K     2
#define HVM_PTE_PGSIZE_256K    3
#define HVM_PTE_PGSIZE_1M      4
#define HVM_PTE_PGSIZE_4M      5
#define HVM_PTE_PGSIZE_16M     6
#define HVM_PTE_PGSIZE_INVALID 7

// PTE bitfield definitions
#define HVM_PTE_PGSIZE_OFF   0
#define HVM_PTE_PGSIZE_WIDTH 3
#define HVM_PTE_U_BIT        5
#define HVM_PTE_CCC_OFF      6
#define HVM_PTE_CCC_WIDTH    3
#define HVM_PTE_R_BIT        9
#define HVM_PTE_W_BIT        10
#define HVM_PTE_X_BIT        11

#define HVM_EXCP_PROT_EX  0x11
/*
 * The VM spec lists 0x14 for "fetch from a page without user access" and
 * leaves 0x12-0x13 reserved, but that is not what implementations report:
 * the Hexagon core raises 0x12 (HEX_CAUSE_FETCH_NO_UPAGE) and the Linux
 * port expects it (HVM_GE_C_XUSER in asm/hexagon_vm.h). The monitor
 * forwards the hardware cause, so 0x12 is the value a guest sees.
 */
#define HVM_EXCP_PROT_UEX 0x12
#define HVM_EXCP_PROT_RD  0x22
#define HVM_EXCP_PROT_WR  0x23
#define HVM_EXCP_PROT_URD 0x24
#define HVM_EXCP_PROT_UWR 0x25

#define HVM_GSR_IE_BIT 30
#define HVM_GSR_UM_BIT 31

#define HVM_INTOP_NOP      0x0
#define HVM_INTOP_GLOBEN   0x1
#define HVM_INTOP_GLOBDIS  0x2
#define HVM_INTOP_LOCEN    0x3
#define HVM_INTOP_LOCDIS   0x4
#define HVM_INTOP_AFFINITY 0x5
#define HVM_INTOP_GET      0x6
#define HVM_INTOP_PEEK     0x7
#define HVM_INTOP_STATUS   0x8
#define HVM_INTOP_POST     0x9
#define HVM_INTOP_CLEAR    0xa

#endif
