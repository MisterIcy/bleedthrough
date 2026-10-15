<?php

function f((A&B)|null $x, (A&B)|(C&D)|false $y): (A&B)|string
{
    return $x;
}
