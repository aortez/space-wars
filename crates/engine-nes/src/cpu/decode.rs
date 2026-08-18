#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AddressingMode {
    Implied,
    Accumulator,
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    Indirect,
    IndirectX,
    IndirectY,
    Relative,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Operation {
    Adc,
    And,
    Asl,
    Bcc,
    Bcs,
    Beq,
    Bit,
    Bmi,
    Bne,
    Bpl,
    Brk,
    Bvc,
    Bvs,
    Clc,
    Cld,
    Cli,
    Clv,
    Cmp,
    Cpx,
    Cpy,
    Dec,
    Dex,
    Dey,
    Eor,
    Inc,
    Inx,
    Iny,
    Jmp,
    Jsr,
    Lda,
    Ldx,
    Ldy,
    Lsr,
    Nop,
    Ora,
    Pha,
    Php,
    Pla,
    Plp,
    Rol,
    Ror,
    Rti,
    Rts,
    Sbc,
    Sec,
    Sed,
    Sei,
    Sta,
    Stx,
    Sty,
    Tax,
    Tay,
    Tsx,
    Txa,
    Txs,
    Tya,
}

impl Operation {
    pub(super) const fn mnemonic(self) -> &'static str {
        match self {
            Self::Adc => "ADC",
            Self::And => "AND",
            Self::Asl => "ASL",
            Self::Bcc => "BCC",
            Self::Bcs => "BCS",
            Self::Beq => "BEQ",
            Self::Bit => "BIT",
            Self::Bmi => "BMI",
            Self::Bne => "BNE",
            Self::Bpl => "BPL",
            Self::Brk => "BRK",
            Self::Bvc => "BVC",
            Self::Bvs => "BVS",
            Self::Clc => "CLC",
            Self::Cld => "CLD",
            Self::Cli => "CLI",
            Self::Clv => "CLV",
            Self::Cmp => "CMP",
            Self::Cpx => "CPX",
            Self::Cpy => "CPY",
            Self::Dec => "DEC",
            Self::Dex => "DEX",
            Self::Dey => "DEY",
            Self::Eor => "EOR",
            Self::Inc => "INC",
            Self::Inx => "INX",
            Self::Iny => "INY",
            Self::Jmp => "JMP",
            Self::Jsr => "JSR",
            Self::Lda => "LDA",
            Self::Ldx => "LDX",
            Self::Ldy => "LDY",
            Self::Lsr => "LSR",
            Self::Nop => "NOP",
            Self::Ora => "ORA",
            Self::Pha => "PHA",
            Self::Php => "PHP",
            Self::Pla => "PLA",
            Self::Plp => "PLP",
            Self::Rol => "ROL",
            Self::Ror => "ROR",
            Self::Rti => "RTI",
            Self::Rts => "RTS",
            Self::Sbc => "SBC",
            Self::Sec => "SEC",
            Self::Sed => "SED",
            Self::Sei => "SEI",
            Self::Sta => "STA",
            Self::Stx => "STX",
            Self::Sty => "STY",
            Self::Tax => "TAX",
            Self::Tay => "TAY",
            Self::Tsx => "TSX",
            Self::Txa => "TXA",
            Self::Txs => "TXS",
            Self::Tya => "TYA",
        }
    }

    pub(super) const fn is_read(self) -> bool {
        matches!(
            self,
            Self::Adc
                | Self::And
                | Self::Bit
                | Self::Cmp
                | Self::Cpx
                | Self::Cpy
                | Self::Eor
                | Self::Lda
                | Self::Ldx
                | Self::Ldy
                | Self::Ora
                | Self::Sbc
        )
    }

    pub(super) const fn is_write(self) -> bool {
        matches!(self, Self::Sta | Self::Stx | Self::Sty)
    }

    pub(super) const fn is_rmw(self) -> bool {
        matches!(
            self,
            Self::Asl | Self::Dec | Self::Inc | Self::Lsr | Self::Rol | Self::Ror
        )
    }

    pub(super) const fn is_branch(self) -> bool {
        matches!(
            self,
            Self::Bcc
                | Self::Bcs
                | Self::Beq
                | Self::Bmi
                | Self::Bne
                | Self::Bpl
                | Self::Bvc
                | Self::Bvs
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Instruction {
    pub operation: Operation,
    pub mode: AddressingMode,
}

const fn instruction(operation: Operation, mode: AddressingMode) -> Instruction {
    Instruction { operation, mode }
}

pub(super) const fn decode(opcode: u8) -> Option<Instruction> {
    use AddressingMode as M;
    use Operation as O;

    Some(match opcode {
        0x00 => instruction(O::Brk, M::Implied),
        0x01 => instruction(O::Ora, M::IndirectX),
        0x05 => instruction(O::Ora, M::ZeroPage),
        0x06 => instruction(O::Asl, M::ZeroPage),
        0x08 => instruction(O::Php, M::Implied),
        0x09 => instruction(O::Ora, M::Immediate),
        0x0a => instruction(O::Asl, M::Accumulator),
        0x0d => instruction(O::Ora, M::Absolute),
        0x0e => instruction(O::Asl, M::Absolute),
        0x10 => instruction(O::Bpl, M::Relative),
        0x11 => instruction(O::Ora, M::IndirectY),
        0x15 => instruction(O::Ora, M::ZeroPageX),
        0x16 => instruction(O::Asl, M::ZeroPageX),
        0x18 => instruction(O::Clc, M::Implied),
        0x19 => instruction(O::Ora, M::AbsoluteY),
        0x1d => instruction(O::Ora, M::AbsoluteX),
        0x1e => instruction(O::Asl, M::AbsoluteX),
        0x20 => instruction(O::Jsr, M::Absolute),
        0x21 => instruction(O::And, M::IndirectX),
        0x24 => instruction(O::Bit, M::ZeroPage),
        0x25 => instruction(O::And, M::ZeroPage),
        0x26 => instruction(O::Rol, M::ZeroPage),
        0x28 => instruction(O::Plp, M::Implied),
        0x29 => instruction(O::And, M::Immediate),
        0x2a => instruction(O::Rol, M::Accumulator),
        0x2c => instruction(O::Bit, M::Absolute),
        0x2d => instruction(O::And, M::Absolute),
        0x2e => instruction(O::Rol, M::Absolute),
        0x30 => instruction(O::Bmi, M::Relative),
        0x31 => instruction(O::And, M::IndirectY),
        0x35 => instruction(O::And, M::ZeroPageX),
        0x36 => instruction(O::Rol, M::ZeroPageX),
        0x38 => instruction(O::Sec, M::Implied),
        0x39 => instruction(O::And, M::AbsoluteY),
        0x3d => instruction(O::And, M::AbsoluteX),
        0x3e => instruction(O::Rol, M::AbsoluteX),
        0x40 => instruction(O::Rti, M::Implied),
        0x41 => instruction(O::Eor, M::IndirectX),
        0x45 => instruction(O::Eor, M::ZeroPage),
        0x46 => instruction(O::Lsr, M::ZeroPage),
        0x48 => instruction(O::Pha, M::Implied),
        0x49 => instruction(O::Eor, M::Immediate),
        0x4a => instruction(O::Lsr, M::Accumulator),
        0x4c => instruction(O::Jmp, M::Absolute),
        0x4d => instruction(O::Eor, M::Absolute),
        0x4e => instruction(O::Lsr, M::Absolute),
        0x50 => instruction(O::Bvc, M::Relative),
        0x51 => instruction(O::Eor, M::IndirectY),
        0x55 => instruction(O::Eor, M::ZeroPageX),
        0x56 => instruction(O::Lsr, M::ZeroPageX),
        0x58 => instruction(O::Cli, M::Implied),
        0x59 => instruction(O::Eor, M::AbsoluteY),
        0x5d => instruction(O::Eor, M::AbsoluteX),
        0x5e => instruction(O::Lsr, M::AbsoluteX),
        0x60 => instruction(O::Rts, M::Implied),
        0x61 => instruction(O::Adc, M::IndirectX),
        0x65 => instruction(O::Adc, M::ZeroPage),
        0x66 => instruction(O::Ror, M::ZeroPage),
        0x68 => instruction(O::Pla, M::Implied),
        0x69 => instruction(O::Adc, M::Immediate),
        0x6a => instruction(O::Ror, M::Accumulator),
        0x6c => instruction(O::Jmp, M::Indirect),
        0x6d => instruction(O::Adc, M::Absolute),
        0x6e => instruction(O::Ror, M::Absolute),
        0x70 => instruction(O::Bvs, M::Relative),
        0x71 => instruction(O::Adc, M::IndirectY),
        0x75 => instruction(O::Adc, M::ZeroPageX),
        0x76 => instruction(O::Ror, M::ZeroPageX),
        0x78 => instruction(O::Sei, M::Implied),
        0x79 => instruction(O::Adc, M::AbsoluteY),
        0x7d => instruction(O::Adc, M::AbsoluteX),
        0x7e => instruction(O::Ror, M::AbsoluteX),
        0x81 => instruction(O::Sta, M::IndirectX),
        0x84 => instruction(O::Sty, M::ZeroPage),
        0x85 => instruction(O::Sta, M::ZeroPage),
        0x86 => instruction(O::Stx, M::ZeroPage),
        0x88 => instruction(O::Dey, M::Implied),
        0x8a => instruction(O::Txa, M::Implied),
        0x8c => instruction(O::Sty, M::Absolute),
        0x8d => instruction(O::Sta, M::Absolute),
        0x8e => instruction(O::Stx, M::Absolute),
        0x90 => instruction(O::Bcc, M::Relative),
        0x91 => instruction(O::Sta, M::IndirectY),
        0x94 => instruction(O::Sty, M::ZeroPageX),
        0x95 => instruction(O::Sta, M::ZeroPageX),
        0x96 => instruction(O::Stx, M::ZeroPageY),
        0x98 => instruction(O::Tya, M::Implied),
        0x99 => instruction(O::Sta, M::AbsoluteY),
        0x9a => instruction(O::Txs, M::Implied),
        0x9d => instruction(O::Sta, M::AbsoluteX),
        0xa0 => instruction(O::Ldy, M::Immediate),
        0xa1 => instruction(O::Lda, M::IndirectX),
        0xa2 => instruction(O::Ldx, M::Immediate),
        0xa4 => instruction(O::Ldy, M::ZeroPage),
        0xa5 => instruction(O::Lda, M::ZeroPage),
        0xa6 => instruction(O::Ldx, M::ZeroPage),
        0xa8 => instruction(O::Tay, M::Implied),
        0xa9 => instruction(O::Lda, M::Immediate),
        0xaa => instruction(O::Tax, M::Implied),
        0xac => instruction(O::Ldy, M::Absolute),
        0xad => instruction(O::Lda, M::Absolute),
        0xae => instruction(O::Ldx, M::Absolute),
        0xb0 => instruction(O::Bcs, M::Relative),
        0xb1 => instruction(O::Lda, M::IndirectY),
        0xb4 => instruction(O::Ldy, M::ZeroPageX),
        0xb5 => instruction(O::Lda, M::ZeroPageX),
        0xb6 => instruction(O::Ldx, M::ZeroPageY),
        0xb8 => instruction(O::Clv, M::Implied),
        0xb9 => instruction(O::Lda, M::AbsoluteY),
        0xba => instruction(O::Tsx, M::Implied),
        0xbc => instruction(O::Ldy, M::AbsoluteX),
        0xbd => instruction(O::Lda, M::AbsoluteX),
        0xbe => instruction(O::Ldx, M::AbsoluteY),
        0xc0 => instruction(O::Cpy, M::Immediate),
        0xc1 => instruction(O::Cmp, M::IndirectX),
        0xc4 => instruction(O::Cpy, M::ZeroPage),
        0xc5 => instruction(O::Cmp, M::ZeroPage),
        0xc6 => instruction(O::Dec, M::ZeroPage),
        0xc8 => instruction(O::Iny, M::Implied),
        0xc9 => instruction(O::Cmp, M::Immediate),
        0xca => instruction(O::Dex, M::Implied),
        0xcc => instruction(O::Cpy, M::Absolute),
        0xcd => instruction(O::Cmp, M::Absolute),
        0xce => instruction(O::Dec, M::Absolute),
        0xd0 => instruction(O::Bne, M::Relative),
        0xd1 => instruction(O::Cmp, M::IndirectY),
        0xd5 => instruction(O::Cmp, M::ZeroPageX),
        0xd6 => instruction(O::Dec, M::ZeroPageX),
        0xd8 => instruction(O::Cld, M::Implied),
        0xd9 => instruction(O::Cmp, M::AbsoluteY),
        0xdd => instruction(O::Cmp, M::AbsoluteX),
        0xde => instruction(O::Dec, M::AbsoluteX),
        0xe0 => instruction(O::Cpx, M::Immediate),
        0xe1 => instruction(O::Sbc, M::IndirectX),
        0xe4 => instruction(O::Cpx, M::ZeroPage),
        0xe5 => instruction(O::Sbc, M::ZeroPage),
        0xe6 => instruction(O::Inc, M::ZeroPage),
        0xe8 => instruction(O::Inx, M::Implied),
        0xe9 => instruction(O::Sbc, M::Immediate),
        0xea => instruction(O::Nop, M::Implied),
        0xec => instruction(O::Cpx, M::Absolute),
        0xed => instruction(O::Sbc, M::Absolute),
        0xee => instruction(O::Inc, M::Absolute),
        0xf0 => instruction(O::Beq, M::Relative),
        0xf1 => instruction(O::Sbc, M::IndirectY),
        0xf5 => instruction(O::Sbc, M::ZeroPageX),
        0xf6 => instruction(O::Inc, M::ZeroPageX),
        0xf8 => instruction(O::Sed, M::Implied),
        0xf9 => instruction(O::Sbc, M::AbsoluteY),
        0xfd => instruction(O::Sbc, M::AbsoluteX),
        0xfe => instruction(O::Inc, M::AbsoluteX),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_exactly_the_151_official_opcodes() {
        assert_eq!(
            (0..=255).filter(|opcode| decode(*opcode).is_some()).count(),
            151
        );
    }
}
