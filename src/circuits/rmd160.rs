// The constraint system matrix for rmd160.

use halo2_proofs::{
    arithmetic::FieldExt,
    circuit::{Cell, Chip, Layouter, Region, AssignedCell, SimpleFloorPlanner},
    plonk::{
        Fixed, Advice, Assignment, Circuit, Column, ConstraintSystem, Error, Expression, Instance,
        Selector,
    },
    poly::Rotation,
};

use std::marker::PhantomData;
use lazy_static::lazy_static;
use rand::rngs::OsRng;
use crate::host::rmd160::gen_modifier_witness;
use std::fmt::format;                                                                                                            
use crate::host::rmd160::{
    ROUNDS_OFFSET,
    PROUNDS_OFFSET,
    R, O, PR, PO
};

struct RMD160Chip<Fp: FieldExt> {
    config: RMD160Config,
    _marker: PhantomData<Fp>,
}

fn field_to_u32<F: FieldExt>(f: &F) -> u32 {
    let mut bytes: Vec<u8> = Vec::new();
    f.write(&mut bytes).unwrap();
    u32::from_le_bytes(bytes.try_into().unwrap())
}

fn u32_to_limbs<Fp: FieldExt>(v: u32) -> [Fp; 8] {
    let mut rem = v;
    let mut r = vec![];
    for _ in 0..8 {
        r.append(&mut vec![Fp::from((rem % 16) as u64)]);
        rem = rem/16;
    }
    r.try_into().unwrap()
}

#[derive(Clone, Debug)]
pub struct RMD160Config {
    limb_col: [Column<Advice>; 5],
    arith_limb_col: [Column<Advice>; 5],
    sum_limb_carry: [Column<Advice>; 2],
    input_limb_col: Column<Advice>,
    limb_index: Column<Fixed>,
    offset: Column<Fixed>,
    shift: Column<Fixed>,
}

impl<Fp: FieldExt> Chip<Fp> for RMD160Chip<Fp> {
    type Config = RMD160Config;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<Fp: FieldExt> RMD160Chip<Fp> {
    fn new(config: RMD160Config) -> Self {
        RMD160Chip {
            config,
            _marker: PhantomData,
        }
    }

    fn configure(cs: &mut ConstraintSystem<Fp>) -> RMD160Config {
        let limb_col = [0; 5]
                .map(|_|cs.advice_column());
        let arith_limb_col = [cs.advice_column(); 5];
        let input_limb_col = cs.advice_column();
        let limb_index = cs.fixed_column();
        let offset = cs.fixed_column();
        let shift = cs.fixed_column();
        let sum_limb_carry = [cs.advice_column(); 2];

        RMD160Config {
            limb_col,
            arith_limb_col,
            input_limb_col,
            limb_index,
            offset,
            sum_limb_carry,
            shift
        }
    }

    fn assign_next(
        &self,
        region: &mut Region<Fp>,
        start_offset: usize,
        previous: &[[AssignedCell<Fp, Fp>; 8]; 5],
        input_limbs: &[Fp; 8],
        round: usize,
        index: usize,
        shift: &[[u32; 16]; 5],
        offset: &[u32; 5],
    ) -> Result<[[AssignedCell<Fp, Fp>; 8]; 5], Error> {
        let mut rotated = [
            previous[0].clone(),
            previous[1].clone(),
            previous[2].clone(),
            previous[3].clone(),
            previous[4].clone(),
        ]; 
        let abcde = [0; 5].map(|i| {
            previous.into_iter().fold(0, |acc, x| 
                field_to_u32(x[i].clone().value().unwrap()) + acc
            )
        });
        let input = self.limbs_to_u32(input_limbs);
        self.assign_input_col(region, start_offset, input_limbs, round, index)?;
        let witness = gen_modifier_witness(&abcde, input, shift[round][index], offset[round]);
        for i in 0..4 {
            let col = self.assign_modifier_witness(
                region,
                start_offset,
                witness[i],
                i,
                round,
            index)?;
            if i == 2 {
                rotated[4] = col;
            } else if i == 3 {
                rotated[1] = col;
            }
        }
        Ok(rotated)
    }

    fn get_init_carry(&self) -> [[AssignedCell<Fp, Fp>; 8]; 5] {
        todo!();
    }

    fn limbs_to_u32(&self, inputs: &[Fp; 8]) -> u32 {
        (*inputs).to_vec().into_iter().fold(0, |acc, x| {
            field_to_u32(&x) + acc
        })
    }

    fn assign_input_col(
        &self,
        region: &mut Region<Fp>,
        start_offset: usize,
        input: &[Fp; 8],
        round: usize,
        index: usize,
    ) -> Result<(), Error> {
        for limb in 0..8 {
            region.assign_advice(
                || format!("input at {}{}", round, index),
                self.config.input_limb_col,
                start_offset + round * 16 + index,
                || Ok(input[limb])
            )?;
        }
        Ok(())
    }

    fn assign_offset_col(
        &self,
        region: &mut Region<Fp>,
        start_offset: usize,
        round: usize,
        index: usize,
        round_offset: &[u32; 5]
    ) -> Result<(), Error> {
        for limb in 0..8 {
            region.assign_fixed(
                || format!("offset at {}{}", round, index),
                self.config.offset,
                (start_offset + round * 16 + index) * 8 + limb,
                || Ok(Fp::from(round_offset[round] as u64))
            )?;
        }
        Ok(())
    }

    fn assign_shift_col(
        &self,
        region: &mut Region<Fp>,
        start_offset: usize,
        round: usize,
        index: usize,
        shift: &[[u32; 16]; 5]
    ) -> Result<(), Error> {
        for limb in 0..8 {
            region.assign_fixed(
                || format!("shift at {}{}", round, index),
                self.config.shift,
                (start_offset + round * 16 + index) * 8 + limb,
                || Ok(Fp::from(shift[round][index] as u64))
            )?;
        }
        Ok(())
    }

    fn assign_modifier_witness(
        &self,
        region: &mut Region<Fp>,
        start_offset: usize,
        w: u32,
        col: usize,
        round: usize,
        index: usize,
    ) -> Result<[AssignedCell<Fp, Fp>; 8], Error> {
        let limbs = u32_to_limbs(w);
        let mut assigned_cells = vec![];
        for limb in 0..8 as usize  {
            let cell = region.assign_advice(
                || format!("input at {}{}", round, index),
                self.config.arith_limb_col[col],
                (start_offset + round * 16 + index) * 8 + limb,
                || Ok(limbs[limb])
            )?;
            assigned_cells.append(&mut vec![cell]);
        }
        Ok(assigned_cells.try_into().unwrap())
    }


    fn assign_rotate(
        &self,
        region: &mut Region<Fp>,
        start_offset: usize,
        limbs: &[[AssignedCell<Fp, Fp>; 8]; 5],
        round: usize,
        index: usize
    ) -> Result<(), Error> {
        for col in 0..5 {
            for limb in 0..8 as usize  {
                let cell = region.assign_advice(
                    || format!("input at {}{}", round, index),
                    self.config.arith_limb_col[col],
                    (start_offset + round * 16 + index) * 8 + limb,
                    || Ok(limbs[col][limb].value().unwrap().clone())
                )?;
                region.constrain_equal(cell.cell(), limbs[col][limb].cell())?;
            }
        }
        Ok(())
    }

    fn rotate_inputs(&self, 
        input_limbs: &[[Fp; 8]; 16],
        round_shift: [usize; 16],
    ) -> [[Fp; 8]; 16] {
        round_shift.map(|i| input_limbs[i].clone())
    }

    fn assign(
        &self,
        layouter: &mut impl Layouter<Fp>,
        input_limbs: [[Fp; 8]; 16],
    ) -> Result<(), Error> {
        let mut r1 = self.get_init_carry();
        let start_offset = 0;
        for round in 0..5 {
            for index in 0..16 {
                layouter.assign_region(
                    || "leaf layer",
                    |mut region| {
                        let row_offset = round*8;
                        self.assign_rotate(
                            &mut region,
                            start_offset,
                            &r1,
                            round,
                            index
                        )?;

                        r1 = self.assign_next(
                            &mut region,
                            start_offset,
                            &r1,
                            &self.rotate_inputs(&input_limbs, O[round])[index],
                            round,
                            index,
                            &R,
                            &ROUNDS_OFFSET
                        )?;

                        self.assign_input_col(
                            &mut region,
                            start_offset,
                            &self.rotate_inputs(&input_limbs, O[round])[index],
                            round,
                            index
                        )?;

                        self.assign_offset_col(
                            &mut region,
                            start_offset,
                            round,
                            index,
                            &ROUNDS_OFFSET
                        )?;

                        self.assign_shift_col(
                            &mut region,
                            start_offset,
                            round,
                            index,
                            &R
                        )?;
                        Ok(())
                    }
                )?;
            }
        }
        let mut r2 = self.get_init_carry();
        let start_offset = 16 * 8 * 5;
        for round in 0..5 {
            for index in 0..16 {
                layouter.assign_region(
                    || "leaf layer",
                    |mut region| {
                        let row_offset = round*8;
                        self.assign_rotate(
                            &mut region,
                            start_offset,
                            &r2,
                            round,
                            index
                        )?;

                        r2 = self.assign_next(
                            &mut region,
                            start_offset,
                            &r2,
                            &self.rotate_inputs(&input_limbs, PO[round])[index],
                            round,
                            index,
                            &PR,
                            &ROUNDS_OFFSET
                        )?;

                        self.assign_input_col(
                            &mut region,
                            start_offset,
                            &self.rotate_inputs(&input_limbs, PO[round])[index],
                            round,
                            index
                        )?;

                        self.assign_offset_col(
                            &mut region,
                            start_offset,
                            round,
                            index,
                            &PROUNDS_OFFSET
                        )?;

                        self.assign_shift_col(
                            &mut region,
                            start_offset,
                            round,
                            index,
                            &R
                        )?;
                        Ok(())
                    }
                )?;
            }
        }
        Ok(())
    }
}



#[cfg(test)]
mod tests {
    use halo2_proofs::pairing::bn256::Fr;
    use halo2_proofs::pairing::group::Group;
    use halo2_proofs::dev::MockProver;

    use halo2_proofs::{
        arithmetic::FieldExt,
        circuit::{Cell, Chip, Layouter, Region, AssignedCell, SimpleFloorPlanner},
        plonk::{
            Fixed, Advice, Assignment, Circuit, Column, ConstraintSystem, Error, Expression, Instance,
            Selector,
        },
    };

    use super::RMD160Chip;                                                                                     
    use super::u32_to_limbs;                                                                                     
    use super::RMD160Config;                                                                                   

    #[derive(Clone, Debug)]
    pub struct HelperChipConfig {
        limb: Column<Advice>
    }

    #[derive(Clone, Debug)]
    pub struct HelperChip {
        config: HelperChipConfig
    }
    
    impl Chip<Fr> for HelperChip {
        type Config = HelperChipConfig;
        type Loaded = ();
    
        fn config(&self) -> &Self::Config {
            &self.config
        }
    
        fn loaded(&self) -> &Self::Loaded {
            &()
        }
    }

    impl HelperChip {
        fn new(config: HelperChipConfig) -> Self {
            HelperChip{
                config,
            }
        }
    
        fn configure(cs: &mut ConstraintSystem<Fr>) -> HelperChipConfig {
            let limb= cs.advice_column();
            HelperChipConfig {
                limb,
            }
        }
    
        fn assign_w(
            &self,
            layouter: &mut impl Layouter<Fr>,
            input_limbs: &[u32; 5],
            offset: usize,
        ) -> Result<[[AssignedCell<Fr, Fr>; 8]; 5], Error> {
            let mut r = vec![];
            for round in 0..5 {
                layouter.assign_region(
                    || "helper layer",
                    |mut region| {
                        let limbs = u32_to_limbs(input_limbs[round]);
                        for limb in 0..8 {
                            r.push(region.assign_advice(
                                || format!("assign w"),
                                self.config.limb,
                                offst + round * 8 + limb,
                                || Ok(limbs[limb])
                            )?);
                        }
                        Ok(())
                    }
                )?;
            }
            todo!()
            /*
            let b = r.chunks(8)
                .map(|x| *x.clone())
                .collect::<Vec<_>>();
            Ok(b.try_into().unwrap())
            */
        }

        fn assign_inputs(
            &self,
            layouter: &mut impl Layouter<Fr>,
            input_limbs: &[u32; 16],
            offset: usize
        ) -> Result<[[AssignedCell<Fr, Fr>; 8]; 16], Error> {
            let mut r = vec![];
            for round in 0..16 {
                layouter.assign_region(
                    || "helper layer",
                    |mut region| {
                        let limbs = u32_to_limbs(input_limbs[round]);
                        for limb in 0..8 {
                            r.push(region.assign_advice(
                                || format!("assign w"),
                                self.config.limb,
                                offst + round * 8 + limb,
                                || Ok(limbs[limb])
                            )?);
                        }
                        Ok(())
                    }
                )?;
            }
            let b = r.chunks(8)
                .collect::<Vec<_>>()
                .into_iter()
                .map(|x| *x)
                .collect::<Vec<_>>();
            Ok(b.try_into().unwrap())
        }

    }

    #[derive(Clone, Debug, Default)]
    struct RMD160Circuit {
        input_limbs: Vec<[Fr; 8]>,
    }
    
    impl Circuit<Fr> for RMD160Circuit {
        type Config = RMD160Config;
        type FloorPlanner = SimpleFloorPlanner;
    
        fn without_witnesses(&self) -> Self {
            Self::default()
        }
    
        fn configure(meta: &mut ConstraintSystem<Fr>) -> Self::Config {
            RMD160Chip::<Fr>::configure(meta)
        }
    
        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<Fr>,
        ) -> Result<(), Error> {
            let chip = RMD160Chip::<Fr>::new(config.clone());
            chip.assign(&mut layouter, self.input_limbs.clone().try_into().unwrap())?;
            println!("synthesize");
            Ok(())
        }
    }


    #[test]
    fn test_rmd160_circuit() {
        let test_circuit = RMD160Circuit {input_limbs: [[Fr::zero();8]; 16].to_vec()} ;
        let prover = MockProver::run(18, &test_circuit, vec![]);
        println!("{:?}", prover);
        prover.unwrap();
    }
}


