<?php

try {
    run();
} catch (FirstException | SecondException $e) {
    report($e);
}
