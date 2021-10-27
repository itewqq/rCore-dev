target remote localhost:1234
# set disassembly-next-line on
layout asm
focus cmd
# break *0x80400058
# break *0x80400146
break *0x0000000080400170
break *0x804002fa
break *0x0000000080400c38