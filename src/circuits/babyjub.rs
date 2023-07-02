use crate::utils::{
    field_to_bn,
    bn_to_field,
};

use crate::circuits::{
    CommonGateConfig,
    Limb,
};

use crate::circuits::range::{
    RangeCheckConfig,
    RangeCheckChip,
};

use std::ops::{Mul, Div};

use halo2_proofs::{
    arithmetic::FieldExt,
    circuit::{Chip, Region},
    plonk::{
        ConstraintSystem, Error
    },
};
use std::marker::PhantomData;

pub struct BabyJubChip<F:FieldExt> {
    config: CommonGateConfig,
    _marker: PhantomData<F>
}



#[derive(Clone, Debug)]
pub struct Point<F: FieldExt> {
    x: Limb<F>,
    y: Limb<F>,
}

/*
impl<F: FieldExt> Point<F> {
}
*/

impl<F: FieldExt> Chip<F> for BabyJubChip<F> {
    type Config = CommonGateConfig;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<F: FieldExt> BabyJubChip<F> {
    pub fn new(config: CommonGateConfig) -> Self {
        BabyJubChip {
            config,
            _marker: PhantomData,
        }
    }

    pub fn configure(cs: &mut ConstraintSystem<F>, range_check_config: &RangeCheckConfig) -> CommonGateConfig {
        CommonGateConfig::configure(cs, range_check_config)
    }

    pub fn add (
        &self,
        region: &mut Region<F>,
        range_check_chip: &mut RangeCheckChip<F>,
        offset: &mut usize,
        lhs: &Point<F>,
        rhs: &Point<F>,
    ) -> Result<Point<F>, Error> {
        /* lambda = dx1x2y1y2
         * x3 = (x1y2 + y1x2)/(1 + lambda)
         * y3 = (y1y2 - x1x2)/(1 - lambda)
         */
        let x1x2 = lhs.x.value * rhs.x.value;
        let y1y2 = lhs.y.value * rhs.y.value;
        let lambda1 = self.config.assign_line(region, range_check_chip, offset,
            [
                Some(lhs.x.clone()),
                None,
                None,
                Some(rhs.x.clone()),
                Some(Limb::new(None, x1x2)),
                None,
            ],
            [None, None, None, None, Some(-F::one()), None, Some(F::one()), None, None],
            0
        )?[4].clone();
        let lambda2 = self.config.assign_line(region, range_check_chip, offset,
            [
                Some(lhs.y.clone()),
                None,
                None,
                Some(rhs.y.clone()),
                Some(Limb::new(None, y1y2)),
                None,
            ],
            [None, None, None, None, Some(-F::one()), None, Some(F::one()), None, None],
            0
        )?[4].clone();
        let lambda = self.config.assign_line(region, range_check_chip, offset,
            [
                Some(lambda1),
                None,
                None,
                Some(lambda2),
                Some(Limb::new(None, y1y2 * x1x2)),
                None,
            ],
            [None, None, None, None, Some(-F::one()), None, Some(F::one()), None, None],
            0
        )?[4].clone();

        let x3_f = lhs.x.value * rhs.y.value + lhs.y.value * rhs.x.value;
        let x3s = self.config.assign_line(region, range_check_chip, offset,
            [
                Some(lhs.x.clone()),
                Some(lhs.y.clone()),
                Some(rhs.x.clone()),
                Some(rhs.y.clone()),
                Some(Limb::new(None, x3_f)),
                None,
            ],
            [None, None, None, None, Some(F::one()), None, Some(F::one()), Some(F::one()), None],
            0
        )?[4].clone();

        //x3 * (1+lambda) = x3s
        let x3_f = x3s.value * (F::one() + lambda.value).invert().unwrap();
        let x3 = self.config.assign_line(region, range_check_chip, offset,
            [
                Some(Limb::new(None, x3_f)),
                Some(x3s.clone()),
                None,
                Some(lambda.clone()),
                None,
                None,
            ],
            [Some(F::one()), Some(-F::one()), None, None, None, None, Some(F::one()), None, None],
            0
        )?[3].clone();



        let y3_f = lhs.y.value * rhs.y.value - lhs.x.value * rhs.x.value;
        let y3s = self.config.assign_line(region, range_check_chip, offset,
            [
                Some(lhs.y.clone()),
                Some(lhs.x.clone()),
                Some(rhs.x.clone()),
                Some(rhs.y.clone()),
                Some(Limb::new(None, y3_f)),
                None,
            ],
            [None, None, None, None, Some(-F::one()), None, Some(F::one()), Some(-F::one()), None],
            0
        )?[4].clone();

        //y3 * (1-lambda) = y3s
        let y3_f = y3s.value * (F::one() - lambda.value).invert().unwrap();
        let y3 = self.config.assign_line(region, range_check_chip, offset,
            [
                Some(Limb::new(None, y3_f)),
                Some(y3s.clone()),
                None,
                Some(lambda.clone()),
                None,
                None,
            ],
            [Some(F::one()), Some(-F::one()), None, None, None, None, Some(-F::one()), None, None],
            0
        )?[3].clone();
        Ok(Point {x: x3, y: y3})
    }

    pub fn mul_scalar(
        &self,
        region: &mut Region<F>,
        range_check_chip: &mut RangeCheckChip<F>,
        offset: &mut usize,
        lhs: &Limb<F>,
        rhs: &Point<F>,
    ) -> Result<Point<F>, Error> {
        todo!()
    }

}
